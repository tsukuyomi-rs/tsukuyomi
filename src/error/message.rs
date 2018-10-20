use http::{Request, StatusCode};
use std::borrow::Cow;
use std::mem;

use crate::input::RequestBody;
use crate::output::ResponseBody;

use super::HttpError;

/// An instance of `HttpError` which holds an HTTP status code and the message string.
#[derive(Debug, failure::Fail)]
#[fail(display = "{}", message)]
pub struct ErrorMessage {
    status: StatusCode,
    message: Cow<'static, str>,
}

impl ErrorMessage {
    /// Creates an `ErrorMessage` from the provided components.
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> ErrorMessage {
        ErrorMessage {
            status,
            message: message.into(),
        }
    }
}

impl HttpError for ErrorMessage {
    fn status(&self) -> StatusCode {
        self.status
    }

    fn body(&mut self, _: &Request<RequestBody>) -> ResponseBody {
        mem::replace(&mut self.message, Default::default()).into()
    }
}

/// A helper function for creating the value of `HttpError` from a string.
///
/// The status code of generated error value is "400 Bad Request".
pub fn bad_request(message: impl Into<Cow<'static, str>>) -> ErrorMessage {
    ErrorMessage::new(StatusCode::BAD_REQUEST, message)
}

/// A helper function for creating the value of `HttpError` from a string.
///
/// The status code of generated error value is "500 Internal Server Error".
pub fn internal_server_error(message: impl Into<Cow<'static, str>>) -> ErrorMessage {
    ErrorMessage::new(StatusCode::INTERNAL_SERVER_ERROR, message)
}
