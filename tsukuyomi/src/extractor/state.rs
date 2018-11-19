//! Extractors for accessing the scope-local data.

use {
    super::Extractor,
    crate::{error::Error, input::State},
};

pub fn by_ref<T>() -> impl Extractor<Output = (State<T>,), Error = Error>
where
    T: Send + Sync + 'static,
{
    super::ready(|input| {
        input
            .state_detached::<T>()
            .ok_or_else(|| crate::error::internal_server_error("missing state"))
    })
}

pub fn clone<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        if let Some(state) = input.state::<T>().cloned() {
            Ok(state)
        } else {
            Err(crate::error::internal_server_error("missing state"))
        }
    })
}
