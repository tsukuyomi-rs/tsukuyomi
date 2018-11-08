//! Extractors for parsing query string.

use serde::de::DeserializeOwned;

use crate::error::{Error, Never};
use crate::extractor::Extractor;

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractQueryError {
    #[fail(display = "missing query string")]
    MissingQuery,

    #[fail(display = "invalid query string: {}", cause)]
    InvalidQuery { cause: failure::Error },
}

pub fn query<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + Send + 'static,
{
    super::ready(|input| {
        if let Some(query_str) = input.uri().query() {
            serde_urlencoded::from_str(query_str).map_err(|cause| {
                crate::error::bad_request(ExtractQueryError::InvalidQuery {
                    cause: cause.into(),
                })
            })
        } else {
            Err(crate::error::bad_request(ExtractQueryError::MissingQuery))
        }
    })
}

pub fn optional<T>() -> impl Extractor<Output = (Option<T>,), Error = Error>
where
    T: DeserializeOwned + Send + 'static,
{
    super::ready(|input| {
        if let Some(query_str) = input.uri().query() {
            serde_urlencoded::from_str(query_str)
                .map(Some)
                .map_err(|cause| {
                    crate::error::bad_request(ExtractQueryError::InvalidQuery {
                        cause: cause.into(),
                    })
                })
        } else {
            Ok(None)
        }
    })
}

pub fn raw() -> impl Extractor<Output = (Option<String>,), Error = Never> {
    super::ready(|input| Ok(input.uri().query().map(ToOwned::to_owned)))
}
