use {
    http::{Response, StatusCode},
    serde_json::json,
    std::fmt,
    tsukuyomi::{
        error::HttpError, //
        handler::AsyncResult,
        input::Input,
        modifier::Modifier,
        output::Output,
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
    fn status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }

    fn to_response(&mut self, _: &mut Input<'_>) -> Output {
        let body = json!({
            "errors": [
                {
                    "message": self.to_string(),
                }
            ],
        }).to_string();
        Response::builder()
            .header("content-type", "application/json")
            .body(body.into())
            .expect("should be a valid response")
    }
}

#[derive(Debug)]
pub struct GraphQLError(Box<dyn HttpError>);

impl fmt::Display for GraphQLError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl HttpError for GraphQLError {
    fn status_code(&self) -> StatusCode {
        self.0.status_code()
    }

    fn to_response(&mut self, _: &mut Input<'_>) -> Output {
        let body = json!({
            "errors": [
                {
                    "message": self.to_string(),
                }
            ],
        }).to_string();
        Response::builder()
            .header("content-type", "application/json")
            .body(body.into())
            .expect("should be a valid response")
    }
}

/// A `Modifier` that catches the all kind of errors from the handler and converts it into a GraphQL error.
#[derive(Debug, Default)]
pub struct GraphQLModifier(());

impl Modifier for GraphQLModifier {
    fn modify(&self, mut handle: AsyncResult<Output>) -> AsyncResult<Output> {
        AsyncResult::poll_fn(move |input| {
            handle.poll_ready(input).map_err(|err| {
                if err.is::<GraphQLParseError>() || err.is::<GraphQLError>() {
                    return err;
                }
                match err.into_http_error() {
                    Ok(e) => GraphQLError(e).into(),
                    Err(crit) => tsukuyomi::Error::critical(crit),
                }
            })
        })
    }
}
