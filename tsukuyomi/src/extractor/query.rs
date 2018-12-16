//! Extractors for parsing query string.

use {
    super::Extractor, //
    crate::{error::Error, util::Never},
    futures01::Future,
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
    Future = impl Future<Item = (T,), Error = Error> + Send + 'static,
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
    Future = impl Future<Item = (Option<T>,), Error = Error> + Send + 'static,
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
    Future = impl Future<Item = (Option<String>,), Error = Never> + Send + 'static,
> {
    super::ready(|input| Ok(input.request.uri().query().map(ToOwned::to_owned)))
}
