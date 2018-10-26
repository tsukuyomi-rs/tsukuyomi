//! Extractors for parsing query string.

use http::StatusCode;
use serde::de::DeserializeOwned;
use std::ops::Deref;

use crate::error::HttpError;
use crate::input::Input;

use super::{FromInput, Preflight};

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractQueryError {
    #[fail(display = "missing query string")]
    MissingQuery,

    #[fail(display = "invalid query string: {}", cause)]
    InvalidQuery { cause: failure::Error },
}

impl HttpError for ExtractQueryError {
    fn status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

/// The instance of `FromInput` which parses the query string in URI.
#[derive(Debug)]
pub struct Query<T>(pub T);

impl<T> Query<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Query<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Query<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = ExtractQueryError;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if let Some(query_str) = input.uri().query() {
            serde_urlencoded::from_str(query_str)
                .map(|x| Preflight::Completed(Query(x)))
                .map_err(|cause| ExtractQueryError::InvalidQuery {
                    cause: cause.into(),
                })
        } else {
            return Err(ExtractQueryError::MissingQuery);
        }
    }
}
