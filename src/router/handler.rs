use futures::{future, Future, IntoFuture};

use context::Context;
use error::Error;
use output::{Output, Responder};

/// [unstable]
/// A trait representing an HTTP handler associated with the certain endpoint.
pub trait Handler {
    /// The type of future which will be returned from `handle`.
    type Future: Future<Item = Output, Error = Error>;

    /// Applies an incoming request to this handler and returns a future.
    fn handle(&self, cx: &Context) -> Self::Future;
}

impl<F, R, T> Handler for F
where
    F: Fn(&Context) -> R,
    R: IntoFuture<Item = T, Error = Error>,
    T: Responder,
{
    type Future = future::AndThen<R::Future, Result<Output, Error>, fn(T) -> Result<Output, Error>>;

    fn handle(&self, cx: &Context) -> Self::Future {
        (*self)(cx)
            .into_future()
            .and_then(|x| Context::with(|cx| x.respond_to(cx)))
    }
}
