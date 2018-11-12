use std::fmt;

use futures::{Async, Future, IntoFuture};
use http::{HttpTryFrom, Method};
use indexmap::IndexSet;

use crate::error::Error;
use crate::extractor::{And, Combine, Extractor, ExtractorExt, Func};
use crate::internal::scoped_map::ScopeId;
use crate::internal::uri::Uri;
use crate::output::Responder;

use super::handler::{AsyncResult, Handler};
use super::{AppResult, ModifierId};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteId(pub(crate) ScopeId, pub(crate) usize);

pub(crate) struct RouteData {
    pub(super) id: RouteId,
    pub(super) uri: Uri,
    pub(super) methods: IndexSet<Method>,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
    pub(super) modifier_ids: Vec<ModifierId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .field("modifier_ids", &self.modifier_ids)
            .finish()
    }
}

/// The type representing a route of HTTP application.
pub struct Route {
    pub(super) methods: IndexSet<Method>,
    pub(super) uri: Uri,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("methods", &self.methods)
            .field("uri", &self.uri)
            .finish()
    }
}

macro_rules! define_route {
    ($($method:ident => $METHOD:ident,)*) => {$(
        pub fn $method<T>(uri: T) -> AppResult<Builder<()>>
        where
            T: AsRef<str>,
        {
            Self::builder()
                .uri(uri)?
                .method(http::Method::$METHOD)
        }
    )*}
}

impl Route {
    /// Creates a builder of this type.
    pub fn builder() -> Builder<()> {
        Builder {
            extractor: (),
            uri: Uri::root(),
            methods: IndexSet::new(),
        }
    }

    #[inline]
    pub fn index() -> Builder<()> {
        Self::builder()
    }

    define_route! {
        get => GET,
        post => POST,
        put => PUT,
        delete => DELETE,
        head => HEAD,
        options => OPTIONS,
        connect => CONNECT,
        patch => PATCH,
        trace => TRACE,
    }
}

/// A builder of `Route`.
#[derive(Debug)]
pub struct Builder<E>
where
    E: Extractor,
{
    extractor: E,
    methods: IndexSet<Method>,
    uri: Uri,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Builder<E>
where
    E: Extractor,
{
    /// Sets the URI of this route.
    pub fn uri<U>(self, uri: U) -> AppResult<Self>
    where
        U: AsRef<str>,
    {
        Ok(Self {
            uri: uri.as_ref().parse()?,
            ..self
        })
    }

    /// Sets the method of this route.
    pub fn method<M>(self, method: M) -> AppResult<Self>
    where
        Method: HttpTryFrom<M>,
    {
        Ok(Self {
            methods: {
                let mut methods = self.methods;
                let method = Method::try_from(method).map_err(Into::into)?;
                methods.insert(method);
                methods
            },
            ..self
        })
    }

    /// Sets the HTTP methods of this route.
    pub fn methods<I, M>(self, methods: I) -> AppResult<Self>
    where
        I: IntoIterator<Item = M>,
        Method: HttpTryFrom<M>,
    {
        Ok(Self {
            methods: {
                let mut orig_methods = self.methods;
                for method in methods {
                    let method = Method::try_from(method).map_err(Into::into)?;
                    orig_methods.insert(method);
                }
                orig_methods
            },
            ..self
        })
    }

    /// Appends an `Extractor` to this builder.
    pub fn with<U>(self, other: U) -> Builder<And<E, U>>
    where
        U: Extractor,
        E::Output: Combine<U::Output> + Send + 'static,
        U::Output: Send + 'static,
    {
        Builder {
            extractor: self.extractor.and(other),
            methods: self.methods,
            uri: self.uri,
        }
    }

    fn finish<F, H>(self, f: F) -> Route
    where
        F: FnOnce(E) -> H,
        H: Handler + Send + Sync + 'static,
    {
        let Self {
            extractor,
            uri,
            methods,
        } = self;

        Route {
            methods,
            uri,
            handler: Box::new(f(extractor)),
        }
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The provided handler always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, handler: F) -> Route
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| {
            super::handler::raw(move |input| match extractor.extract(input) {
                Err(e) => AsyncResult::ready(Err(e.into())),
                Ok(future) => {
                    let handler = handler.clone();
                    let mut future = future.map(move |arg| handler.call(arg));
                    AsyncResult::polling(move |input| {
                        futures::try_ready!(crate::input::with_set_current(input, || future
                            .poll()
                            .map_err(Into::into))).respond_to(input)
                        .map(|response| Async::Ready(response.map(Into::into)))
                        .map_err(Into::into)
                    })
                }
            })
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The result of provided handler is returned by `Future`.
    pub fn handle<F, R>(self, handler: F) -> Route
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| {
            super::handler::raw(move |input| match extractor.extract(input) {
                Err(e) => AsyncResult::ready(Err(e.into())),
                Ok(future) => {
                    let handler = handler.clone();
                    let mut future = future
                        .map_err(Into::into)
                        .and_then(move |arg| handler.call(arg).into_future());
                    AsyncResult::polling(move |input| {
                        futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                            .respond_to(input)
                            .map(|response| Async::Ready(response.map(Into::into)))
                            .map_err(Into::into)
                    })
                }
            })
        })
    }
}

impl Builder<()> {
    pub fn raw<H>(self, handler: H) -> Route
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| handler)
    }
}

#[macro_export(local_inner_macros)]
macro_rules! route {
    ($uri:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            route_expr_impl!($uri);
        }
        __Dummy::route()
    }};
    ($uri:expr, methods = [$($methods:expr),*]) => {
        route!($uri)
            $( .method($methods).expect("should be validate by proc-macro") )*
    };
    () => ( $crate::route() );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> Builder<impl Extractor<Output = (u32, String)>> {
        Route::get("/:id/:name")
            .unwrap()
            .with(crate::extractor::param::pos(0))
            .with(crate::extractor::param::pos(1))
    }

    #[test]
    #[ignore]
    fn compiletest1() {
        drop(
            crate::app::App::builder()
                .route(generated().reply(|id: u32, name: String| {
                    drop((id, name));
                    "dummy"
                })) //
                .finish()
                .expect("failed to construct App"),
        );
    }

    #[test]
    #[ignore]
    fn compiletest2() {
        drop(
            crate::app::App::builder()
                .route(generated().with(crate::extractor::body::plain()).reply(
                    |id: u32, name: String, body: String| {
                        drop((id, name, body));
                        "dummy"
                    },
                )) //
                .finish()
                .expect("failed to construct App"),
        );
    }
}
