//! Extractors for accessing the request-local data.

use {
    super::Extractor,
    crate::{error::Error, localmap::LocalKey},
};

pub fn remove<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
where
    T: Send + 'static,
{
    super::ready(move |input| {
        input
            .locals
            .remove(key)
            .ok_or_else(|| crate::error::internal_server_error("missing local value"))
    })
}

pub fn clone<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + 'static,
{
    super::ready(move |input| {
        input
            .locals
            .get(key)
            .cloned()
            .ok_or_else(|| crate::error::internal_server_error("missing local value"))
    })
}
