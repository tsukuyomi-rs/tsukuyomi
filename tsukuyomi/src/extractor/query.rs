//! Extractors for parsing query string.

use {
    super::Extractor, //
    crate::{core::Never, error::Error},
    serde::de::DeserializeOwned,
};

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractQueryError {
    #[fail(display = "missing query string")]
    MissingQuery,

    #[fail(display = "invalid query string: {}", cause)]
    InvalidQuery { cause: failure::Error },
}

pub fn query<T>() -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Future = futures01::future::FutureResult<(T,), Error>,
>
where
    T: DeserializeOwned + Send + 'static,
{
    super::ready(|input| {
        if let Some(query_str) = input.request.uri().query() {
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

pub fn optional<T>() -> impl Extractor<
    Output = (Option<T>,), //
    Error = Error,
    Future = futures01::future::FutureResult<(Option<T>,), Error>,
>
where
    T: DeserializeOwned + Send + 'static,
{
    super::ready(|input| {
        if let Some(query_str) = input.request.uri().query() {
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

pub fn raw() -> impl Extractor<
    Output = (Option<String>,), //
    Error = Never,
    Future = futures01::future::FutureResult<(Option<String>,), Never>,
> {
    super::ready(|input| Ok(input.request.uri().query().map(ToOwned::to_owned)))
}
