use http::Method;
use std::fmt;

use error::Error;
use future::{ready, Future, Poll};
use input::Input;
use output::{Output, Responder};

use super::uri::Uri;

enum HandlerKind {
    Ready(Box<Fn(&mut Input) -> Result<Output, Error> + Send + Sync>),
    Async(Box<Fn() -> Box<Future<Output = Result<Output, Error>> + Send> + Send + Sync>),
}

impl fmt::Debug for HandlerKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HandlerKind::Ready(..) => f.debug_tuple("Ready").finish(),
            HandlerKind::Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

pub(crate) type Handle = Box<Future<Output = Result<Output, Error>> + Send>;

/// A type representing an endpoint.
///
/// The value of this type contains a `Handler` to handle the accepted HTTP request,
/// and some information for constructing a `Router`.
#[derive(Debug)]
pub struct Endpoint {
    uri: Uri,
    method: Method,
    handler: HandlerKind,
}

impl Endpoint {
    pub(super) fn new_ready<R>(
        uri: Uri,
        method: Method,
        handler: impl Fn(&mut Input) -> R + Send + Sync + 'static,
    ) -> Endpoint
    where
        R: Responder,
    {
        Endpoint {
            uri: uri,
            method: method,
            handler: HandlerKind::Ready(Box::new(move |input| handler(input).respond_to(input))),
        }
    }

    pub(super) fn new_async<R>(uri: Uri, method: Method, handler: impl Fn() -> R + Send + Sync + 'static) -> Endpoint
    where
        R: Future + Send + 'static,
        R::Output: Responder,
    {
        Endpoint {
            uri: uri,
            method: method,
            handler: HandlerKind::Async(Box::new(move || Box::new(HandlerFuture(handler())))),
        }
    }

    /// Returns the full HTTP path of this endpoint.
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Returns the reference to `Method` which this route allows.
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn handle(&self) -> Handle {
        match self.handler {
            HandlerKind::Ready(ref f) => Box::new(ready(Input::with_mut(|input| f(input)))),
            HandlerKind::Async(ref f) => f(),
        }
    }
}

#[derive(Debug)]
struct HandlerFuture<F>(F);

impl<F> Future for HandlerFuture<F>
where
    F: Future,
    F::Output: Responder,
{
    type Output = Result<Output, Error>;

    fn poll(&mut self) -> Poll<Self::Output> {
        Input::with(|cx| self.0.poll().map(|x| x.respond_to(cx)))
    }
}
