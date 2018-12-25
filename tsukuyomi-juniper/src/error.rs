use {
    http::{Request, Response, StatusCode},
    serde_json::json,
    std::fmt,
    tsukuyomi::{
        error::{Error, HttpError}, //
        future::{Poll, TryFuture},
        handler::{AllowedMethods, Handler, ModifyHandler},
        input::Input,
        output::ResponseBody,
    },
};

#[derive(Debug)]
pub enum GraphQLParseError {
    InvalidRequestMethod,
    MissingQuery,
    MissingMime,
    InvalidMime,
    ParseJson(serde_json::Error),
    ParseQuery(serde_urlencoded::de::Error),
    DecodeUtf8(std::str::Utf8Error),
}

impl fmt::Display for GraphQLParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphQLParseError::InvalidRequestMethod => f.write_str("the request method is invalid"),
            GraphQLParseError::MissingQuery => f.write_str("missing query"),
            GraphQLParseError::MissingMime => f.write_str("missing content-type"),
            GraphQLParseError::InvalidMime => f.write_str("the content type is invalid."),
            GraphQLParseError::ParseJson(ref e) => e.fmt(f),
            GraphQLParseError::ParseQuery(ref e) => e.fmt(f),
            GraphQLParseError::DecodeUtf8(ref e) => e.fmt(f),
        }
    }
}

impl HttpError for GraphQLParseError {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        let body = json!({
            "errors": [
                {
                    "message": self.to_string(),
                }
            ],
        })
        .to_string();
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("content-type", "application/json")
            .body(body)
            .expect("should be a valid response")
    }
}

#[derive(Debug)]
pub struct GraphQLError(Error);

impl fmt::Display for GraphQLError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl HttpError for GraphQLError {
    type Body = ResponseBody;

    fn into_response(self, request: &Request<()>) -> Response<Self::Body> {
        let body = json!({
            "errors": [
                {
                    "message": self.to_string(),
                }
            ],
        })
        .to_string();

        let mut response = self.0.into_response(request).map(|_| body.into());

        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::header::HeaderValue::from_static("application/json"),
        );

        response
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
    type Output = H::Output;
    type Handler = GraphQLHandler<H>; // private;

    fn modify(&self, inner: H) -> Self::Handler {
        GraphQLHandler { inner }
    }
}

#[derive(Debug)]
pub struct GraphQLHandler<H> {
    inner: H,
}

impl<H> Handler for GraphQLHandler<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = Error;
    type Handle = GraphQLHandle<H::Handle>;

    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        self.inner.allowed_methods()
    }

    fn handle(&self) -> Self::Handle {
        GraphQLHandle {
            inner: self.inner.handle(),
        }
    }
}

#[derive(Debug)]
pub struct GraphQLHandle<H> {
    inner: H,
}

impl<H> TryFuture for GraphQLHandle<H>
where
    H: TryFuture,
{
    type Ok = H::Ok;
    type Error = Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        self.inner.poll_ready(input).map_err(|err| {
            let err = err.into();
            if err.is::<GraphQLParseError>() || err.is::<GraphQLError>() {
                err
            } else {
                GraphQLError(err).into()
            }
        })
    }
}
