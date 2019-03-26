use {
    http::{Response, StatusCode},
    serde_json::json,
    tsukuyomi::{
        error::{Error, HttpError}, //
        future::{Async, Poll, TryFuture},
        handler::{metadata::Metadata, Handler, ModifyHandler},
        input::Input,
        output::IntoResponse,
        util::Either,
    },
};

#[derive(Debug, failure::Fail)]
pub enum GraphQLParseError {
    #[fail(display = "the request method is invalid")]
    InvalidRequestMethod,
    #[fail(display = "missing query")]
    MissingQuery,
    #[fail(display = "missing content-type")]
    MissingMime,
    #[fail(display = "the content type is invalid.")]
    InvalidMime,
    #[fail(display = "failed to parse input as a JSON object")]
    ParseJson(#[fail(cause)] serde_json::Error),
    #[fail(display = "failed to parse HTTP query")]
    ParseQuery(#[fail(cause)] serde_urlencoded::de::Error),
    #[fail(display = "failed to decode input as a UTF-8 sequence")]
    DecodeUtf8(#[fail(cause)] std::str::Utf8Error),
}

impl HttpError for GraphQLParseError {
    fn status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

/// Creates a `ModifyHandler` that catches the all kind of errors that the handler throws
/// and converts them into GraphQL errors.
pub fn capture_errors() -> CaptureErrors {
    CaptureErrors(())
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct CaptureErrors(());

impl<H> ModifyHandler<H> for CaptureErrors
where
    H: Handler,
{
    type Output = Either<Response<String>, H::Output>;
    type Error = Error;
    type Handler = CaptureErrorsHandler<H>; // private;

    fn modify(&self, inner: H) -> Self::Handler {
        CaptureErrorsHandler { inner }
    }
}

#[derive(Debug)]
pub struct CaptureErrorsHandler<H> {
    inner: H,
}

impl<H> Handler for CaptureErrorsHandler<H>
where
    H: Handler,
{
    type Output = Either<Response<String>, H::Output>;
    type Error = Error;
    type Handle = CaptureErrorsHandle<H::Handle>;

    fn metadata(&self) -> Metadata {
        self.inner.metadata()
    }

    fn handle(&self) -> Self::Handle {
        CaptureErrorsHandle {
            inner: self.inner.handle(),
        }
    }
}

#[derive(Debug)]
pub struct CaptureErrorsHandle<H> {
    inner: H,
}

impl<H> TryFuture for CaptureErrorsHandle<H>
where
    H: TryFuture,
{
    type Ok = Either<Response<String>, H::Ok>;
    type Error = Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        match self.inner.poll_ready(input) {
            Ok(Async::Ready(ok)) => Ok(Async::Ready(Either::Right(ok))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                let err = err.into();
                let body = json!({
                    "errors": [
                        {
                            "message": err.to_string(),
                        }
                    ],
                })
                .to_string();

                let mut response = err.into_response().map(|_| body.into());
                response.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    http::header::HeaderValue::from_static("application/json"),
                );

                Ok(Async::Ready(Either::Left(response)))
            }
        }
    }
}
