use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::{Async, Future, IntoFuture};
use indexmap::IndexSet;

use crate::error::Error;
use crate::extractor::{And, Combine, Extractor, ExtractorExt, Func};
use crate::fs::NamedFile;
use crate::internal::uri::Uri;
use crate::output::Responder;

use super::handler::{AsyncResult, Handler};

#[doc(hidden)]
pub use http::Method;

/// Creates a builder of this type.
pub fn builder() -> Builder<()> {
    Builder {
        extractor: (),
        uri: Uri::root(),
        methods: IndexSet::new(),
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
    pub fn uri(self, uri: Uri) -> Self {
        Self { uri, ..self }
    }

    /// Sets the method of this route.
    pub fn method(self, method: Method) -> Self {
        Self {
            methods: {
                let mut methods = self.methods;
                methods.insert(method);
                methods
            },
            ..self
        }
    }

    /// Sets the HTTP methods of this route.
    pub fn methods<I>(self, methods: I) -> Self
    where
        I: IntoIterator<Item = Method>,
    {
        Self {
            methods: {
                let mut orig_methods = self.methods;
                orig_methods.extend(methods);
                orig_methods
            },
            ..self
        }
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

    fn finish<F, H>(self, f: F) -> impl RouteConfig
    where
        F: FnOnce(E) -> H,
        H: Handler + Send + Sync + 'static,
    {
        move |cx: &mut RouteContext| {
            let handler = f(self.extractor);
            cx.methods(self.methods);
            cx.uri(self.uri);
            cx.handler(handler);
        }
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The provided handler always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, handler: F) -> impl RouteConfig
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
    pub fn handle<F, R>(self, handler: F) -> impl RouteConfig
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

impl<E> Builder<E>
where
    E: Extractor<Output = ()>,
{
    pub fn serve_file(self, path: impl AsRef<Path>) -> impl RouteConfig {
        #[derive(Clone)]
        #[allow(missing_debug_implementations)]
        struct ArcPath(Arc<PathBuf>);

        impl AsRef<Path> for ArcPath {
            fn as_ref(&self) -> &Path {
                (*self.0).as_ref()
            }
        }

        let arc_path = ArcPath(Arc::new(path.as_ref().to_path_buf()));

        self.handle(move || NamedFile::open(arc_path.clone()).map_err(Into::into))
    }
}

impl Builder<()> {
    pub fn raw<H>(self, handler: H) -> impl RouteConfig
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| handler)
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct RouteContext {
    pub(super) uri: Uri,
    pub(super) methods: Option<IndexSet<Method>>,
    pub(super) handler: Option<Box<dyn Handler + Send + Sync + 'static>>,
}

impl RouteContext {
    fn uri(&mut self, uri: Uri) {
        self.uri = uri;
    }

    fn methods<I>(&mut self, methods: I)
    where
        I: IntoIterator<Item = Method>,
    {
        self.methods = Some(methods.into_iter().collect());
    }

    fn handler<H>(&mut self, handler: H)
    where
        H: Handler + Send + Sync + 'static,
    {
        self.handler = Some(Box::new(handler));
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait RouteConfig {
    fn configure(self, cx: &mut RouteContext);
}

impl<F> RouteConfig for F
where
    F: FnOnce(&mut RouteContext),
{
    fn configure(self, cx: &mut RouteContext) {
        self(cx)
    }
}

#[macro_export(local_inner_macros)]
macro_rules! route {
    (
        $uri:expr
            $(
                , method = $METHOD:ident
            )*
            $(
                , methods = [$($METHODS:ident),*]
            )*
    ) => {{
        use $crate::app::route::Method;
        enum __Dummy {}
        impl __Dummy {
            route_expr_impl!($uri);
        }
        __Dummy::route()
            $( .method(Method::$METHOD) )*
            $( .methods(__tsukuyomi_vec![$(Method::$METHODS),*]) )*
    }};
    () => ( $crate::app::route::builder() );
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tsukuyomi_vec {
    ($($t:tt)*) => (vec![$($t)*]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> Builder<impl Extractor<Output = (u32, String)>> {
        builder()
            .uri("/:id/:name".parse().unwrap())
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
