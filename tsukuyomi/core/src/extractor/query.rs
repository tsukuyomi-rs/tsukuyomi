//! Extractors for parsing query string.

use std::fmt;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractQueryError {
    #[fail(display = "missing query string")]
    MissingQuery,

    #[fail(display = "invalid query string: {}", cause)]
    InvalidQuery { cause: failure::Error },
}

pub fn query<T>() -> Query<T>
where
    T: DeserializeOwned + 'static,
{
    Query::default()
}

pub struct Query<T>(PhantomData<fn() -> T>);

impl<T> Default for Query<T> {
    fn default() -> Self {
        Query(PhantomData)
    }
}

impl<T> fmt::Debug for Query<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryExtractor").finish()
    }
}

impl<T> Extractor for Query<T>
where
    T: DeserializeOwned + 'static,
{
    type Output = (T,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        if let Some(query_str) = input.uri().query() {
            serde_urlencoded::from_str(query_str)
                .map(|out| Extract::Ready((out,)))
                .map_err(|cause| {
                    crate::error::bad_request(ExtractQueryError::InvalidQuery {
                        cause: cause.into(),
                    })
                })
        } else {
            return Err(crate::error::bad_request(ExtractQueryError::MissingQuery));
        }
    }
}
