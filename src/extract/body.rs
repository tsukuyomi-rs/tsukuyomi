//! Extractors for parsing message body.

use std::ops::Deref;
use std::str;

use bytes::Bytes;
use http::StatusCode;
use mime::Mime;
use serde::de::DeserializeOwned;

use crate::error::HttpError;
use crate::input::Input;

use super::{FromInput, Preflight};

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
pub enum ExtractBodyError {
    #[fail(display = "missing the header field `Content-type`")]
    MissingContentType,

    #[fail(
        display = "the header field `Content-type` is not an expected value (expected: {})",
        expected
    )]
    UnexpectedContentType { expected: &'static str },

    #[fail(display = "the header field `Content-type` is not a valid MIME")]
    InvalidMime,

    #[fail(display = "charset in `Content-type` must be equal to `utf-8`")]
    NotUtf8Charset,

    #[fail(
        display = "the content of message body is invalid: {}",
        cause
    )]
    InvalidContent { cause: failure::Error },
}

impl HttpError for ExtractBodyError {
    fn status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

fn get_mime_opt<'a>(input: &'a mut Input<'_>) -> Result<Option<&'a Mime>, ExtractBodyError> {
    crate::input::header::content_type(input).map_err(|_| ExtractBodyError::InvalidMime)
}

fn get_mime<'a>(input: &'a mut Input<'_>) -> Result<&'a Mime, ExtractBodyError> {
    get_mime_opt(input)?.ok_or_else(|| ExtractBodyError::MissingContentType)
}

/// The instance of `FromInput` which parses the message body as an UTF-8 string
/// and converts it into a value by using `serde_plain`.
#[derive(Debug)]
pub struct Plain<T = String>(pub T);

impl<T> Plain<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl AsRef<str> for Plain<String> {
    #[cfg_attr(tarpaulin, skip)]
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<T> Deref for Plain<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Plain<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = ExtractBodyError;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if let Some(mime) = get_mime_opt(input)? {
            if mime.type_() != mime::TEXT || mime.subtype() != mime::PLAIN {
                return Err(ExtractBodyError::UnexpectedContentType {
                    expected: "text/plain",
                });
            }
            if let Some(charset) = mime.get_param("charset") {
                if charset != "utf-8" {
                    return Err(ExtractBodyError::NotUtf8Charset);
                }
            }
        }
        Ok(Preflight::Incomplete(()))
    }

    fn finalize(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        let s = str::from_utf8(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
            cause: cause.into(),
        })?;
        serde_plain::from_str(s)
            .map(Plain)
            .map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
    }
}

/// The instance of `FromInput` which deserializes the message body
/// into a JSON value by using `serde_json`.
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Json<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = ExtractBodyError;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let mime = get_mime(input)?;
        if *mime != mime::APPLICATION_JSON {
            return Err(ExtractBodyError::UnexpectedContentType {
                expected: "application/json",
            });
        }
        Ok(Preflight::Incomplete(()))
    }

    fn finalize(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        serde_json::from_slice(&*data)
            .map(Json)
            .map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
    }
}

/// The instance of `FromInput` which deserializes the message body
/// into a value by using `serde_urlencoded`.
#[derive(Debug)]
pub struct Urlencoded<T>(pub T);

impl<T> Urlencoded<T> {
    #[allow(missing_docs)]
    #[cfg_attr(tarpaulin, skip)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Urlencoded<T> {
    type Target = T;

    #[cfg_attr(tarpaulin, skip)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Urlencoded<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = ExtractBodyError;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let mime = get_mime(input)?;
        if *mime != mime::APPLICATION_WWW_FORM_URLENCODED {
            return Err(ExtractBodyError::UnexpectedContentType {
                expected: "application/x-www-form-urlencoded",
            });
        }
        Ok(Preflight::Incomplete(()))
    }

    fn finalize(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        serde_urlencoded::from_bytes(&*data)
            .map(Urlencoded)
            .map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
    }
}
