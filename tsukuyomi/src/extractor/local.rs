//! Extractors for accessing the request-local data.

use {super::Extractor, crate::input::localmap::LocalKey};

pub fn remove<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,)>
where
    T: Send + 'static,
{
    super::lazy(move |_| {
        let key = key;
        crate::future::poll_fn(move |cx| {
            cx.input
                .locals
                .remove(key)
                .map(Into::into)
                .ok_or_else(missing_local_value)
        })
    })
}

pub fn clone<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,)>
where
    T: Clone + Send + 'static,
{
    super::lazy(move |_| {
        let key = key;
        crate::future::poll_fn(move |cx| {
            cx.input
                .locals
                .get(key)
                .cloned()
                .map(Into::into)
                .ok_or_else(missing_local_value)
        })
    })
}

fn missing_local_value() -> crate::Error {
    crate::error::internal_server_error("missing local value")
}
