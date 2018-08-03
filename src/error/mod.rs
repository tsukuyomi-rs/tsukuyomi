//! Components for constructing and handling HTTP errors.

pub mod handler;

use failure::{self, Fail};
use http::{Request, Response, StatusCode};
use std::{error, fmt};

use input::RequestBody;
use output::ResponseBody;

/// A type alias representing a critical error.
pub type CritError = Box<dyn error::Error + Send + Sync + 'static>;

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = ::std::result::Result<T, Error>;

/// [unstable]
/// A trait representing HTTP errors.
pub trait HttpError: fmt::Debug + fmt::Display + Send + 'static {
    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode;

    /// Returns the representation as a `Fail`, if possible.
    fn as_fail(&self) -> Option<&dyn Fail> {
        None
    }

    /// Convert itself into an HTTP response.
    #[allow(unused_variables)]
    fn to_response(&self, request: &Request<RequestBody>) -> Option<Response<ResponseBody>> {
        None
    }
}

/// A type which holds all kinds of errors occurring in handlers.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    Boxed(Box<dyn HttpError>),
    Crit(CritError),
}

impl<E> From<E> for Error
where
    E: HttpError,
{
    fn from(err: E) -> Error {
        Error {
            kind: ErrorKind::Boxed(Box::new(err)),
        }
    }
}

impl Error {
    /// Creates an HTTP error from an error value and an HTTP status code.
    pub fn from_failure<E>(err: E, status: StatusCode) -> Error
    where
        E: Into<failure::Error>,
    {
        Error::from(Failure::new(status, err))
    }

    /// Creates an HTTP error representing "404 Not Found".
    pub fn not_found() -> Error {
        Error::from_failure(format_err!("Not Found"), StatusCode::NOT_FOUND)
    }

    /// Creates an HTTP error representing "405 Method Not Allowed".
    pub fn method_not_allowed() -> Error {
        Error::from_failure(
            format_err!("Method Not Allowed"),
            StatusCode::METHOD_NOT_ALLOWED,
        )
    }

    /// Creates an HTTP error representing "400 Bad Request" from the provided error value.
    pub fn bad_request<E>(e: E) -> Error
    where
        E: Into<failure::Error>,
    {
        Error::from_failure(e, StatusCode::BAD_REQUEST)
    }

    /// Creates an HTTP error representing "500 Internal Server Error", from the provided error value .
    pub fn internal_server_error<E>(e: E) -> Error
    where
        E: Into<failure::Error>,
    {
        Error::from_failure(e, StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// Creates a *critical* error from an error value.
    ///
    /// The word "critical" means that the error will not be converted into an HTTP response.
    /// If the framework receives this kind of error, it wlll abort the current connection abruptly
    /// without sending an HTTP response.
    ///
    /// See [the documentation at hyper][hyper-service-error] for details.
    ///
    /// [hyper-service-error]:
    /// https://docs.rs/hyper/0.12.*/hyper/service/trait.Service.html#associatedtype.Error
    pub fn critical<E>(err: E) -> Error
    where
        E: Into<CritError>,
    {
        Error {
            kind: ErrorKind::Crit(err.into()),
        }
    }

    /// Returns `true` if this error is a *critical* error.
    pub fn is_critical(&self) -> bool {
        match self.kind {
            ErrorKind::Crit(..) => true,
            _ => false,
        }
    }

    /// Returns the representation as `HttpError` of this error value.
    ///
    /// If the value is a criticial error, it will return a `None`.
    pub fn as_http_error(&self) -> Option<&dyn HttpError> {
        match self.kind {
            ErrorKind::Boxed(ref e) => Some(&**e),
            ErrorKind::Crit(..) => None,
        }
    }

    #[allow(missing_docs)]
    pub fn try_into_http_error(self) -> ::std::result::Result<Box<dyn HttpError>, CritError> {
        match self.kind {
            ErrorKind::Boxed(e) => Ok(e),
            ErrorKind::Crit(e) => Err(e),
        }
    }

    #[cfg(test)]
    pub(crate) fn status_code(&self) -> Option<StatusCode> {
        self.as_http_error().map(|e| e.status_code())
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Failure {
    status: StatusCode,
    err: failure::Error,
}

impl Failure {
    /// Create a new `Failure` from the specified HTTP status code and an error value.
    pub fn new(status: StatusCode, err: impl Into<failure::Error>) -> Failure {
        Failure {
            err: err.into(),
            status,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.err, f)
    }
}

impl HttpError for Failure {
    fn status_code(&self) -> StatusCode {
        self.status
    }

    fn as_fail(&self) -> Option<&dyn Fail> {
        Some(self.err.as_fail())
    }
}

/// A helper type emulating the standard never_type (`!`).
#[derive(Debug)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        unreachable!()
    }
}

impl Fail for Never {}

impl HttpError for Never {
    fn status_code(&self) -> StatusCode {
        unreachable!()
    }
}
