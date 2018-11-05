use std::fmt;
use std::sync::Arc;

use futures::{Async, Future, IntoFuture};
use http::{HttpTryFrom, Method};

use crate::error::Error;
use crate::extractor::{And, Combine, Extract, Extractor, ExtractorExt, Func};
use crate::handler::{Handle, Handler};
use crate::output::Responder;
use crate::recognizer::uri::Uri;

use super::{AppError, AppResult, ModifierId, ScopeId};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteId(pub(crate) ScopeId, pub(crate) usize);

pub(super) struct RouteData {
    pub(super) id: RouteId,
    pub(super) uri: Uri,
    pub(super) method: Method,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
    pub(super) modifier_ids: Vec<ModifierId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("method", &self.method)
            .field("modifier_ids", &self.modifier_ids)
            .finish()
    }
}

/// The type representing a route of HTTP application.
#[derive(Debug)]
pub struct Route {
    pub(super) inner: AppResult<RouteInner>,
}

pub(super) struct RouteInner {
    pub(super) method: Method,
    pub(super) uri: Uri,
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
}

impl fmt::Debug for RouteInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteInner")
            .field("method", &self.method)
            .field("uri", &self.uri)
            .finish()
    }
}

impl Route {
    /// Creates a builder of this type.
    pub fn builder() -> Builder<()> {
        Builder::new(())
    }
}

/// A builder of `Route`.
#[derive(Debug)]
pub struct Builder<E>
where
    E: Extractor,
{
    extractor: E,
    method: http::Result<Method>,
    uri: failure::Fallible<Uri>,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Builder<E>
where
    E: Extractor,
{
    pub(crate) fn new(extractor: E) -> Self {
        Builder {
            extractor,
            uri: Ok(Uri::root()),
            method: Ok(Method::GET),
        }
    }

    /// Sets the URI of this route.
    pub fn uri<U>(self, uri: U) -> Self
    where
        U: AsRef<str>,
    {
        Builder {
            uri: self.uri.and_then(|_| uri.as_ref().parse()),
            ..self
        }
    }

    /// Sets the method of this route.
    pub fn method<M>(self, method: M) -> Self
    where
        Method: HttpTryFrom<M>,
    {
        Builder {
            method: self
                .method
                .and_then(|_| Method::try_from(method).map_err(Into::into)),
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
            method: self.method,
            uri: self.uri,
        }
    }

    fn finish<F, R>(self, f: F) -> Route
    where
        F: FnOnce(E) -> R,
        R: Fn(&mut crate::input::Input<'_>) -> Handle + Send + Sync + 'static,
    {
        let Self {
            extractor,
            uri,
            method,
        } = self;

        let method = match method {
            Ok(method) => method,
            Err(err) => {
                return Route {
                    inner: Err(AppError::from_failure(err)),
                }
            }
        };

        let uri = match uri {
            Ok(uri) => uri,
            Err(err) => {
                return Route {
                    inner: Err(AppError::from_failure(err)),
                }
            }
        };

        Route {
            inner: Ok(RouteInner {
                method,
                uri,
                handler: Box::new(crate::handler::raw(f(extractor))),
            }),
        }
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The provided handler always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, handler: F) -> Route
    where
        F: Func<E::Output> + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| {
            let handler = Arc::new(handler);
            move |input| match extractor.extract(input) {
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
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The result of provided handler is returned by `Future`.
    pub fn handle<F, R>(self, handler: F) -> Route
    where
        F: Func<E::Output, Out = R> + Send + Sync + 'static,
        R: IntoFuture<Error = Error> + 'static,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| {
            let handler = Arc::new(handler);
            move |input| match extractor.extract(input) {
                Ok(Extract::Ready(arg)) => {
                    let mut future = handler.call(arg).into_future();
                    Handle::polling(move |input| {
                        futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                            .respond_to(input)
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
                        futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                            .respond_to(input)
                            .map(|response| Async::Ready(response.map(Into::into)))
                            .map_err(Into::into)
                    })
                }
            }
        })
    }
}
