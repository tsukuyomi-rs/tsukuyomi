//! Extractors for accessing HTTP header fields.

use http::header::{HeaderName, HeaderValue};
use mime::Mime;

use crate::error::Error;
use crate::extractor::Extractor;

pub trait FromHeaderValue: Sized + 'static {
    type Error: Into<Error>;

    fn from_header_value(h: &HeaderValue) -> Result<Self, Self::Error>;
}

impl FromHeaderValue for String {
    type Error = Error;

    #[inline]
    fn from_header_value(h: &HeaderValue) -> Result<Self, Self::Error> {
        Self::from_utf8(h.as_bytes().to_vec()).map_err(crate::error::bad_request)
    }
}

pub fn header<T>(name: HeaderName) -> impl Extractor<Output = (T,), Error = Error>
where
    T: FromHeaderValue + Send,
{
    super::ready(move |input| match input.headers().get(&name) {
        Some(h) => T::from_header_value(h).map_err(Into::into),
        None => Err(crate::error::bad_request(format!(
            "missing header field: {}",
            name
        ))),
    })
}

pub fn exact<T>(name: HeaderName, value: T) -> impl Extractor<Output = (), Error = Error>
where
    T: PartialEq<HeaderValue> + Send + Sync + 'static,
{
    super::validate(move |input| match input.headers().get(&name) {
        Some(h) if value.eq(h) => Ok(()),
        Some(..) => Err(crate::error::bad_request(format!(
            "mismatched header field: {}",
            name
        ))),
        None => Err(crate::error::bad_request(format!(
            "missing header field: {}",
            name
        ))),
    })
}

/// Creates an extractor which parses the header field `Content-type`.
pub fn content_type() -> impl Extractor<Output = (Mime,), Error = Error> {
    super::ready(|input| match crate::input::header::content_type(input)? {
        Some(mime) => Ok(mime.clone()),
        None => Err(crate::error::bad_request("missing Content-type")),
    })
}
