//! Extractors for parsing message body.

use {
    crate::{
        error::Error,
        extractor::Extractor,
        input::{body::RequestBody, header::ContentType},
    },
    bytes::Bytes,
    futures01::Future,
    mime::Mime,
    serde::de::DeserializeOwned,
    std::str,
};

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

    #[fail(display = "the content of message body is invalid: {}", cause)]
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
    T: Send + 'static,
    D: self::decode::Decoder<T>,
{
    super::raw(move |input| {
        if let Err(err) = {
            crate::input::header::parse::<ContentType>(input) //
                .and_then(|mime_opt| {
                    decoder
                        .validate_mime(mime_opt)
                        .map_err(crate::error::bad_request)
                })
        } {
            return futures01::future::Either::A(futures01::future::err(err));
        }

        let read_all = match input.locals.remove(&RequestBody::KEY) {
            Some(body) => body.read_all(),
            None => return futures01::future::Either::A(futures01::future::err(stolen_payload())),
        };

        futures01::future::Either::B(read_all.from_err().and_then(|data| {
            D::decode(&data)
                .map(|out| (out,))
                .map_err(crate::error::bad_request)
        }))
    })
}

#[inline]
pub fn plain<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded(self::decode::PlainTextDecoder::default())
}

#[inline]
pub fn json<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded(self::decode::JsonDecoder::default())
}

#[inline]
pub fn urlencoded<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded(self::decode::UrlencodedDecoder::default())
}

pub fn read_all() -> impl Extractor<Output = (Bytes,), Error = Error> {
    super::raw(|input| match input.locals.remove(&RequestBody::KEY) {
        Some(body) => {
            futures01::future::Either::A(body.read_all().map(|x| (x,)).map_err(Into::into))
        }
        None => futures01::future::Either::B(futures01::future::err(stolen_payload())),
    })
}

pub fn stream() -> impl Extractor<Output = (RequestBody,), Error = Error> {
    super::ready(|input| match input.locals.remove(&RequestBody::KEY) {
        Some(body) => Ok(body),
        None => Err(stolen_payload()),
    })
}

fn stolen_payload() -> crate::error::Error {
    crate::error::internal_server_error("The instance of raw RequestBody has already stolen.")
}
