//! Extractors for parsing message body.

use {
    super::Extractor,
    crate::{error::Error, future::TryFuture, input::body::RequestBody},
    bytes::Bytes,
    futures01::Future,
    mime::Mime,
    serde::de::DeserializeOwned,
    std::{marker::PhantomData, str},
};

#[derive(Debug, failure::Fail)]
enum ExtractBodyError {
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

trait Decoder<T> {
    fn validate_mime(mime: Option<&Mime>) -> Result<(), ExtractBodyError>;
    fn decode(data: &Bytes) -> Result<T, ExtractBodyError>;
}

#[derive(Debug, Default)]
struct PlainTextDecoder(());

impl<T> Decoder<T> for PlainTextDecoder
where
    T: DeserializeOwned,
{
    fn validate_mime(mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
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
struct JsonDecoder(());

impl<T> Decoder<T> for JsonDecoder
where
    T: DeserializeOwned,
{
    fn validate_mime(mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
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
struct UrlencodedDecoder(());

impl<T> Decoder<T> for UrlencodedDecoder
where
    T: DeserializeOwned,
{
    fn validate_mime(mime: Option<&Mime>) -> Result<(), ExtractBodyError> {
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

fn decoded<T, D>() -> self::decoded::Decoded<T, D>
where
    D: Decoder<T>,
{
    self::decoded::Decoded {
        _marker: PhantomData,
    }
}

mod decoded {
    use {
        crate::{
            error::Error,
            extractor::Extractor,
            future::{Poll, TryFuture},
            input::{body::RequestBody, header::ContentType, Input},
        },
        futures01::Future,
        std::marker::PhantomData,
    };

    #[derive(Debug)]
    pub(super) struct Decoded<T, D> {
        pub(super) _marker: PhantomData<fn(D) -> T>,
    }

    impl<T, D> Extractor for Decoded<T, D>
    where
        D: super::Decoder<T>,
    {
        type Output = (T,);
        type Error = Error;
        type Extract = DecodedFuture<T, D>;

        fn extract(&self) -> Self::Extract {
            DecodedFuture {
                state: State::Init,
                _marker: PhantomData,
            }
        }
    }

    #[allow(missing_debug_implementations)]
    enum State {
        Init,
        ReadAll(crate::input::body::ReadAll),
    }

    #[allow(missing_debug_implementations)]
    pub(super) struct DecodedFuture<T, D> {
        state: State,
        _marker: PhantomData<fn(D) -> T>,
    }

    impl<T, D> TryFuture for DecodedFuture<T, D>
    where
        D: super::Decoder<T>,
    {
        type Ok = (T,);
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                self.state = match self.state {
                    State::Init => {
                        let mime_opt = crate::input::header::parse::<ContentType>(input)?;
                        D::validate_mime(mime_opt).map_err(crate::error::bad_request)?;

                        let read_all = match input.locals.remove(&RequestBody::KEY) {
                            Some(body) => body.read_all(),
                            None => return Err(super::stolen_payload()),
                        };
                        State::ReadAll(read_all)
                    }
                    State::ReadAll(ref mut read_all) => {
                        let data = futures01::try_ready!(read_all.poll());
                        return D::decode(&data)
                            .map(|out| (out,).into())
                            .map_err(crate::error::bad_request);
                    }
                };
            }
        }
    }
}

#[inline]
pub fn plain<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded::<T, PlainTextDecoder>()
}

#[inline]
pub fn json<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded::<T, JsonDecoder>()
}

#[inline]
pub fn urlencoded<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + Send + 'static,
{
    self::decoded::<T, UrlencodedDecoder>()
}

pub fn read_all() -> impl Extractor<
    Output = (Bytes,),
    Error = Error,
    Extract = impl TryFuture<Ok = (Bytes,), Error = Error> + Send + 'static,
> {
    super::extract(|| {
        let mut read_all: Option<crate::input::body::ReadAll> = None;
        crate::future::poll_fn(move |input| loop {
            if let Some(ref mut read_all) = read_all {
                return read_all
                    .poll()
                    .map(|x| x.map(|bytes| (bytes,)))
                    .map_err(Into::into);
            }
            read_all = Some(
                input
                    .locals
                    .remove(&RequestBody::KEY)
                    .ok_or_else(stolen_payload)?
                    .read_all(),
            );
        })
    })
}

pub fn stream() -> impl Extractor<
    Output = (RequestBody,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (RequestBody,), Error = Error> + Send + 'static,
> {
    super::extract(|| {
        crate::future::poll_fn(|input| {
            input
                .locals
                .remove(&RequestBody::KEY)
                .map(|body| (body,).into())
                .ok_or_else(stolen_payload)
        })
    })
}

fn stolen_payload() -> crate::error::Error {
    crate::error::internal_server_error("The instance of raw RequestBody has already stolen.")
}
