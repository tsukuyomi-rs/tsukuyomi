use futures::{future, Future, IntoFuture};

use context::Context;
use error::Error;
use output::{Output, Responder};

pub trait Handler {
    type Future: Future<Item = Output, Error = Error>;

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
