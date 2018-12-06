//! Extractors for parsing message body.

use {
    crate::{
        error::Error,
        extractor::Extractor,
        future::{Async, MaybeFuture},
        input::body::RequestBody,
    },
    bytes::Bytes,
    futures01::Future as _Future01,
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

fn decoded<T, D>(decoder: D) -> impl Extractor<Output = (T,)>
where
    T: 'static,
    D: self::decode::Decoder<T> + Send + Sync + 'static,
{
    super::raw(move |input| {
        if let Err(err) = input.content_type().and_then(|mime_opt| {
            decoder
                .validate_mime(mime_opt)
                .map_err(crate::error::bad_request)?;
            Ok(mime_opt)
        }) {
            return MaybeFuture::err(err);
        }

        input.body().map_or_else(
            || MaybeFuture::err(stolen_payload()),
            |body| {
                let mut read_all = body.read_all();
                MaybeFuture::from(crate::future::Compat01::new(futures01::future::poll_fn(
                    move || {
                        let data = futures01::try_ready!(read_all.poll().map_err(Error::critical));
                        D::decode(&data)
                            .map(|out| Async::Ready((out,)))
                            .map_err(crate::error::bad_request)
                    },
                )))
            },
        )
    })
}

#[inline]
pub fn plain<T>() -> impl Extractor<Output = (T,)>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::PlainTextDecoder::default())
}

#[inline]
pub fn json<T>() -> impl Extractor<Output = (T,)>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::JsonDecoder::default())
}

#[inline]
pub fn urlencoded<T>() -> impl Extractor<Output = (T,)>
where
    T: DeserializeOwned + 'static,
{
    self::decoded(self::decode::UrlencodedDecoder::default())
}

pub fn raw() -> impl Extractor<Output = (Bytes,)> {
    super::raw(|input| {
        input.body().map_or_else(
            || MaybeFuture::err(stolen_payload()),
            |body| {
                MaybeFuture::from(crate::future::Compat01::new(
                    body.read_all().map(|out| (out,)).map_err(Error::critical),
                ))
            },
        )
    })
}

pub fn stream() -> impl Extractor<Output = (RequestBody,)> {
    super::ready(|input| input.body().ok_or_else(stolen_payload))
}

fn stolen_payload() -> crate::error::Error {
    crate::error::internal_server_error("The instance of raw RequestBody has already stolen.")
}
