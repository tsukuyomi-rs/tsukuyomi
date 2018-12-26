//! Extractors for parsing message body.

use {
    super::Extractor,
    crate::{
        error::Error,
        future::{Poll, TryFuture},
        input::{body::RequestBody, header::ContentType, localmap::LocalData, Input},
    },
    bytes::Bytes,
    futures01::{Future, Stream},
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
    fn decode(data: &[u8]) -> Result<T, ExtractBodyError>;
}

fn decode<T, D>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: 'static,
    D: Decoder<T> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Decode<T, D> {
        _marker: PhantomData<fn(D) -> T>,
    }

    impl<T, D> Extractor for Decode<T, D>
    where
        D: Decoder<T>,
    {
        type Output = (T,);
        type Error = Error;
        type Extract = DecodeFuture<T, D>;

        fn extract(&self) -> Self::Extract {
            DecodeFuture {
                state: State::Init,
                _marker: PhantomData,
            }
        }
    }

    #[allow(missing_debug_implementations)]
    enum State {
        Init,
        ReadAll(futures01::stream::Concat2<RequestBody>),
    }

    #[allow(missing_debug_implementations)]
    struct DecodeFuture<T, D> {
        state: State,
        _marker: PhantomData<fn(D) -> T>,
    }

    impl<T, D> TryFuture for DecodeFuture<T, D>
    where
        D: Decoder<T>,
    {
        type Ok = (T,);
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                self.state = match self.state {
                    State::Init => {
                        let mime_opt = crate::input::header::parse::<ContentType>(input)?;
                        D::validate_mime(mime_opt).map_err(crate::error::bad_request)?;
                        RequestBody::take_from(input.locals)
                            .map(|body| State::ReadAll(body.concat2()))
                            .ok_or_else(stolen_payload)?
                    }
                    State::ReadAll(ref mut read_all) => {
                        let data = futures01::try_ready!(read_all.poll());
                        return D::decode(&*data)
                            .map(|out| (out,).into())
                            .map_err(crate::error::bad_request);
                    }
                };
            }
        }
    }

    Decode::<T, D> {
        _marker: PhantomData,
    }
}

/// Creates an `Extractor` that parses the entire of request body into `T` as a plain text.
pub fn plain<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + 'static,
{
    #[allow(missing_debug_implementations)]
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

        fn decode(data: &[u8]) -> Result<T, ExtractBodyError> {
            let s = str::from_utf8(&*data) //
                .map_err(|cause| ExtractBodyError::InvalidContent {
                    cause: cause.into(),
                })?;
            serde_plain::from_str(s) //
                .map_err(|cause| ExtractBodyError::InvalidContent {
                    cause: cause.into(),
                })
        }
    }

    decode::<T, PlainTextDecoder>()
}

/// Creates an `Extractor` that parses the entire of request body into `T` as JSON data.
pub fn json<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + 'static,
{
    #[allow(missing_debug_implementations)]
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

        fn decode(data: &[u8]) -> Result<T, ExtractBodyError> {
            serde_json::from_slice(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    decode::<T, JsonDecoder>()
}

/// Creates an `Extractor` that parses the entire of request body into `T` as url-encoded data.
pub fn urlencoded<T>() -> impl Extractor<
    Output = (T,),
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned + 'static,
{
    #[allow(missing_debug_implementations)]
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

        fn decode(data: &[u8]) -> Result<T, ExtractBodyError> {
            serde_urlencoded::from_bytes(&*data).map_err(|cause| ExtractBodyError::InvalidContent {
                cause: cause.into(),
            })
        }
    }

    decode::<T, UrlencodedDecoder>()
}

/// Creates an extractor that reads the entire of request body as a single byte sequence.
pub fn read_all() -> impl Extractor<
    Output = (Bytes,),
    Error = Error,
    Extract = impl TryFuture<Ok = (Bytes,), Error = Error> + Send + 'static,
> {
    super::extract(|| {
        let mut read_all: Option<futures01::stream::Concat2<RequestBody>> = None;
        crate::future::poll_fn(move |input| loop {
            if let Some(ref mut read_all) = read_all {
                return read_all
                    .poll()
                    .map(|x| x.map(|chunk| (chunk.into_bytes(),)))
                    .map_err(Into::into);
            }
            read_all = Some(
                RequestBody::take_from(input.locals)
                    .ok_or_else(stolen_payload)?
                    .concat2(),
            );
        })
    })
}

/// Creates an `Extractor` that takes the raw instance of request body.
pub fn stream() -> impl Extractor<
    Output = (RequestBody,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (RequestBody,), Error = Error> + Send + 'static,
> {
    super::extract(|| {
        crate::future::poll_fn(|input| {
            RequestBody::take_from(input.locals)
                .map(|body| (body,).into())
                .ok_or_else(stolen_payload)
        })
    })
}

fn stolen_payload() -> crate::error::Error {
    crate::error::internal_server_error("The instance of raw RequestBody has already stolen.")
}
