//! Extractors for accessing the protocol extensions.

use {
    crate::{
        error::Error, //
        extractor::Extractor,
    },
    futures01::Future,
};

pub fn clone<T>() -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Future = impl Future<Item = (T,), Error = Error> + Send + 'static,
>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        input
            .request
            .extensions()
            .get()
            .cloned()
            .ok_or_else(|| crate::error::internal_server_error("missing extension"))
    })
}
