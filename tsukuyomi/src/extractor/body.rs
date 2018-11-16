//! Extractors for parsing message body.

use std::str;

use bytes::Bytes;
use futures::{Async, Future};
use mime::Mime;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::extractor::{ExtractStatus, Extractor};
use crate::input::body::RequestBody;

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

mod decode {
    use super::*;

    pub trait Decoder<T> {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError>;
        fn decode(data: &Bytes) -> Result<T, ExtractBodyError>;
    }

    #[derive(Debug, Default)]
    pub struct PlainTextDecoder(());

    impl<T> Decoder<T> for PlainTextDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            if let Some(mime) = mime {
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
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            let s = str::from_utf8(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })?;
            serde_plain::from_str(s).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    #[derive(Debug, Default)]
    pub struct JsonDecoder(());

    impl<T> Decoder<T> for JsonDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            let mime = mime.ok_or_else(|| ExtractBodyError::MissingContentType)?;
            if *mime != mime::APPLICATION_JSON {
                return Err(ExtractBodyError::UnexpectedContentType {
                    expected: "application/json",
                });
            }
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            serde_json::from_slice(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    #[derive(Debug, Default)]
    pub struct UrlencodedDecoder(());

    impl<T> Decoder<T> for UrlencodedDecoder
    where
        T: DeserializeOwned + 'static,
    {
        fn validate_mime(&self, mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
            let mime = mime.ok_or_else(|| ExtractBodyError::MissingContentType)?;
            if *mime != mime::APPLICATION_WWW_FORM_URLENCODED {
                return Err(ExtractBodyError::UnexpectedContentType {
                    expected: "application/x-www-form-urlencoded",
                });
            }
            Ok(())
        }

        fn decode(data: &Bytes) -> Result<T, ExtractBodyError> {
            serde_urlencoded::from_bytes(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }
}

fn decoded<T, D>(decoder: D) -> impl Extractor<Output = (T,), Error = Error>
where
    T: 'static,
    D: self::decode::Decoder<T> + Send + Sync + 'static,
{
    super::raw(move |input| {
        {
            let mime_opt = input.content_type()?;
            decoder
                .validate_mime(mime_opt)
                .map_err(crate::error::bad_request)?;
        }

        input
            .read_all()
            .ok_or_else(stolen_payload)
            .map(|mut read_all| {
                ExtractStatus::Pending(futures::future::poll_fn(move || {
                    let data = futures::try_ready!(read_all.poll().map_err(Error::critical));
                    D::decode(&data)
                        .map(|out| Async::Ready((out,)))
                        .map_err(crate::error::bad_request)
                }))
            })
    })
}

#[inline]
pub fn plain<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::PlainTextDecoder::default())
}

#[inline]
pub fn json<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::JsonDecoder::default())
}

#[inline]
pub fn urlencoded<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::UrlencodedDecoder::default())
}

pub fn raw() -> impl Extractor<Output = (Bytes,), Error = Error> {
    super::raw(|input| {
        input
            .read_all()
            .map(|future| ExtractStatus::Pending(future.map(|out| (out,)).map_err(Error::critical)))
            .ok_or_else(stolen_payload)
    })
}

pub fn stream() -> impl Extractor<Output = (RequestBody,), Error = Error> {
    super::ready(|input| input.take_body().ok_or_else(stolen_payload))
}

fn stolen_payload() -> crate::error::Error {
    crate::error::internal_server_error("The instance of raw RequestBody has already stolen.")
}
