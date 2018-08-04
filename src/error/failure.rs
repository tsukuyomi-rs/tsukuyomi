use failure::Error;
use http::StatusCode;

use super::HttpError;

/// An instance of `HttpError` which associates HTTP status code with a value of `failure::Error`.
#[derive(Debug, Fail)]
#[fail(display = "{}", err)]
pub struct Failure {
    status: StatusCode,
    err: Error,
}

impl Failure {
    /// Create a new `Failure` from the specified HTTP status code and an error value.
    pub fn new(status: StatusCode, err: impl Into<Error>) -> Failure {
        Failure {
            err: err.into(),
            status,
        }
    }

    /// Creates an HTTP error representing "400 Bad Request" from the provided error value.
    pub fn bad_request(err: impl Into<Error>) -> Failure {
        Failure::new(StatusCode::BAD_REQUEST, err)
    }

    /// Creates an HTTP error representing "500 Internal Server Error", from the provided error value .
    pub fn internal_server_error(err: impl Into<Error>) -> Failure {
        Failure::new(StatusCode::INTERNAL_SERVER_ERROR, err)
    }
}

impl HttpError for Failure {
    fn status(&self) -> StatusCode {
        self.status
    }
}
