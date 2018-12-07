//! Extractors for accessing the protocol extensions.

use crate::{extractor::Extractor, future::Async};

pub fn clone<T>() -> impl Extractor<Output = (T,)>
where
    T: Clone + Send + Sync + 'static,
{
    super::lazy(|_| {
        crate::future::poll_fn(|cx| {
            cx.input
                .request
                .extensions()
                .get()
                .cloned()
                .map(Async::Ready)
                .ok_or_else(|| crate::error::internal_server_error("missing extension"))
        })
    })
}
