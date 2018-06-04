use futures::{Future, IntoFuture};
use http::Response;
use std::sync::Arc;

use context::Context;
use error::Error;
use response::ResponseBody;

pub trait Handler {
    type Future: Future<Item = Response<ResponseBody>, Error = Error>;

    fn handle_async(&self, cx: &Context) -> Self::Future;
}

impl<H: Handler> Handler for Box<H> {
    type Future = H::Future;

    fn handle_async(&self, cx: &Context) -> Self::Future {
        (**self).handle_async(cx)
    }
}

impl<H: Handler> Handler for Arc<H> {
    type Future = H::Future;

    fn handle_async(&self, cx: &Context) -> Self::Future {
        (**self).handle_async(cx)
    }
}

#[derive(Debug)]
pub struct HandlerFn<F>(F);

impl<F, R> Handler for HandlerFn<F>
where
    F: Fn(&Context) -> R,
    R: IntoFuture<Item = Response<ResponseBody>, Error = Error>,
{
    type Future = R::Future;

    fn handle_async(&self, cx: &Context) -> Self::Future {
        (self.0)(cx).into_future()
    }
}

pub fn handler_fn<F, R>(f: F) -> HandlerFn<F>
where
    F: Fn(&Context) -> R,
    R: IntoFuture<Item = Response<ResponseBody>, Error = Error>,
{
    HandlerFn(f)
}
