#![allow(missing_docs)]

use futures::{Async, Future, IntoFuture};
use http::Method;
use std::sync::Arc;

use crate::app::builder::{Route as RouteContext, RouteConfig};
use crate::error::Error;
use crate::extractor::{And, Combine, Extract, Extractor, ExtractorExt, Func};
use crate::handler::Handle;
use crate::output::Responder;

macro_rules! define_route {
    ($($method:ident => $METHOD:ident,)*) => {$(
        pub fn $method(uri: impl Into<String>) -> Route {
            Route::new(Method::$METHOD, uri, ())
        }

        #[macro_export(local_inner_macros)]
        macro_rules! $method {
            ($uri:expr) => {{
                struct __Dummy;
                impl __Dummy {
                    route_impl!($method $uri);
                }
                __Dummy::route()
            }};
        }
    )*}
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

// Equivalent to `get("/")`
pub fn index() -> Route {
    self::get("/")
}

#[derive(Debug)]
pub struct Route<E = ()>
where
    E: Extractor,
{
    extractor: E,
    uri: String,
    method: Method,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Route<E>
where
    E: Extractor,
{
    pub fn new(method: Method, uri: impl Into<String>, extractor: E) -> Self {
        Self {
            extractor,
            uri: uri.into(),
            method,
        }
    }

    pub fn uri(self, uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            ..self
        }
    }

    pub fn method(self, method: Method) -> Self {
        Self { method, ..self }
    }

    pub fn with<U>(self, other: U) -> Route<And<E, U>>
    where
        U: Extractor,
        E::Output: Combine<U::Output> + Send + 'static,
        U::Output: Send + 'static,
    {
        Route {
            extractor: self.extractor.and(other),
            method: self.method,
            uri: self.uri,
        }
    }

    pub fn reply<F>(self, handler: F) -> impl RouteConfig
    where
        F: Func<E::Output> + Send + Sync + 'static,
        F::Out: Responder,
    {
        move |route: &mut RouteContext<'_>| {
            let Self {
                extractor,
                uri,
                method,
            } = self;

            let handler = Arc::new(handler);

            route.uri(&uri);
            route.method(method);
            route.handler(crate::handler::raw(move |input| {
                match extractor.extract(input) {
                    Ok(Extract::Ready(arg)) => {
                        let x = handler.call(arg);
                        let result = x
                            .respond_to(input)
                            .map(|response| response.map(Into::into))
                            .map_err(Into::into);
                        Handle::ready(result)
                    }
                    Err(e) => Handle::ready(Err(e.into())),
                    Ok(Extract::Incomplete(future)) => {
                        let handler = handler.clone();
                        let mut future = future.map(move |arg| handler.call(arg));
                        Handle::polling(move |input| {
                            futures::try_ready!(crate::input::with_set_current(input, || future
                                .poll()
                                .map_err(Into::into))).respond_to(input)
                            .map(|response| Async::Ready(response.map(Into::into)))
                            .map_err(Into::into)
                        })
                    }
                }
            }));
        }
    }

    pub fn handle<F, R>(self, handler: F) -> impl RouteConfig
    where
        F: Func<E::Output, Out = R> + Send + Sync + 'static,
        R: IntoFuture<Error = Error> + 'static,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        move |route: &mut RouteContext<'_>| {
            let Self {
                extractor,
                uri,
                method,
            } = self;

            let handler = Arc::new(handler);

            route.uri(&uri);
            route.method(method);
            route.handler(crate::handler::raw(move |input| {
                match extractor.extract(input) {
                    Ok(Extract::Ready(arg)) => {
                        let mut future = handler.call(arg).into_future();
                        Handle::polling(move |input| {
                            futures::try_ready!(
                                crate::input::with_set_current(input, || future.poll())
                            ).respond_to(input)
                            .map(|response| Async::Ready(response.map(Into::into)))
                            .map_err(Into::into)
                        })
                    }
                    Err(e) => Handle::ready(Err(e.into())),
                    Ok(Extract::Incomplete(future)) => {
                        let handler = handler.clone();
                        let mut future = future
                            .map_err(Into::into)
                            .and_then(move |arg| handler.call(arg));
                        Handle::polling(move |input| {
                            futures::try_ready!(
                                crate::input::with_set_current(input, || future.poll())
                            ).respond_to(input)
                            .map(|response| Async::Ready(response.map(Into::into)))
                            .map_err(Into::into)
                        })
                    }
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> Route<impl Extractor<Output = (u32, String)>> {
        Route::new(Method::GET, "/:id/:name", ())
            .with(crate::extractor::param::pos(0))
            .with(crate::extractor::param::pos(1))
    }

    #[test]
    #[ignore]
    fn compiletest1() {
        let route = generated().reply(|id: u32, name: String| {
            drop((id, name));
            "dummy"
        });

        let app = crate::app::App::builder()
            .route(route)
            .finish()
            .expect("failed to construct App");
        drop(app);
    }

    #[test]
    #[ignore]
    fn compiletest2() {
        let route = generated().with(crate::extractor::body::plain()).reply(
            |id: u32, name: String, body: String| {
                drop((id, name, body));
                "dummy"
            },
        );

        let app = crate::app::App::builder()
            .route(route)
            .finish()
            .expect("failed to construct App");
        drop(app);
    }
}
