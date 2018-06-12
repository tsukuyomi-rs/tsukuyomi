use context::Context;
use error::Error;
use future::{Future, Poll};
use output::{Output, Responder};

/// [unstable]
/// A trait representing an HTTP handler associated with the certain endpoint.
pub trait Handler {
    /// The type of future which will be returned from `handle`.
    type Future: Future<Output = Result<Output, Error>>;

    /// Applies an incoming request to this handler and returns a future.
    fn handle(&self, cx: &Context) -> Self::Future;
}

impl<F, R, T> Handler for F
where
    F: Fn(&Context) -> R,
    R: Future<Output = T>,
    T: Responder,
{
    type Future = HandlerFuture<R>;

    fn handle(&self, cx: &Context) -> Self::Future {
        HandlerFuture((*self)(cx))
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct HandlerFuture<F>(F);

impl<F, T> Future for HandlerFuture<F>
where
    F: Future<Output = T>,
    T: Responder,
{
    type Output = Result<Output, Error>;

    fn poll(&mut self) -> Poll<Self::Output> {
        let item = ready!(self.0.poll());
        Context::with(|cx| Poll::Ready(item.respond_to(cx.request())))
    }
}
