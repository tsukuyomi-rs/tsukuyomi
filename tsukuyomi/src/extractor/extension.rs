//! Extractors for accessing the protocol extensions.

use crate::extractor::Extractor;

pub fn clone<T>() -> impl Extractor<Output = (T,)>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        if let Some(ext) = input.request.extensions().get::<T>() {
            Ok(ext.clone())
        } else {
            Err(crate::error::internal_server_error("missing extension"))
        }
    })
}
