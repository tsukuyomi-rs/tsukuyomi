//! Extractors for accessing the scope-local data.

use {super::Extractor, crate::error::Error};

pub fn clone<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        input
            .states
            .try_get::<T>()
            .cloned()
            .ok_or_else(|| crate::error::internal_server_error(""))
    })
}
