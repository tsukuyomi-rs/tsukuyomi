use futures::Future;
use http::Method;
use std::fmt;

use context::Context;
use error::Error;
use handler::Handler;
use output::Output;

pub struct Route {
    path: String,
    method: Method,
    handler: Box<Fn(&Context) -> Box<Future<Item = Output, Error = Error> + Send> + Send + Sync + 'static>,
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
            handler: Box::new(move |cx| {
                // TODO: specialization for Result<T, E>
                Box::new(handler.handle(cx))
            }),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn handle(&self, cx: &Context) -> Box<Future<Item = Output, Error = Error> + Send> {
        (*self.handler)(cx)
    }
}
