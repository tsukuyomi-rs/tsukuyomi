use futures::{future, Future, IntoFuture};
use http::Method;
use std::fmt;

use context::Context;
use error::Error;
use response::{Output, Responder};

use super::context::RouterContext;

pub struct Route {
    path: String,
    method: Method,
    handler: Box<
        Fn(&Context, &mut RouterContext) -> Box<Future<Item = Output, Error = Error> + Send>
            + Send
            + Sync
            + 'static,
    >,
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Route")
            .field("path", &self.path)
            .field("method", &self.method)
            .finish()
    }
}

impl Route {
    pub fn new<H>(path: &str, method: Method, handler: H) -> Route
    where
        H: Handler + Send + Sync + 'static,
        H::Future: Send + 'static,
    {
        Route {
            path: path.to_owned(),
            method: method,
            handler: Box::new(move |cx, rcx| {
                // TODO: specialization for Result<T, E>
                Box::new(handler.handle(cx, rcx))
            }),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn handle(
        &self,
        cx: &Context,
        rcx: &mut RouterContext,
    ) -> Box<Future<Item = Output, Error = Error> + Send> {
        (*self.handler)(cx, rcx)
    }
}

pub trait Handler {
    type Future: Future<Item = Output, Error = Error>;

    fn handle(&self, cx: &Context, rcx: &mut RouterContext) -> Self::Future;
}

impl<F, R, T> Handler for F
where
    F: Fn(&Context, &mut RouterContext) -> R,
    R: IntoFuture<Item = T, Error = Error>,
    T: Responder,
{
    type Future = future::AndThen<R::Future, Result<Output, Error>, fn(T) -> Result<Output, Error>>;

    fn handle(&self, cx: &Context, rcx: &mut RouterContext) -> Self::Future {
        (*self)(cx, rcx)
            .into_future()
            .and_then(|x| Context::with(|cx| x.respond_to(cx)))
    }
}
