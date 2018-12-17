//! Extractors for accessing the protocol extensions.

use crate::{
    error::Error, //
    extractor::Extractor,
    future::TryFuture,
};

pub fn clone<T>() -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
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
