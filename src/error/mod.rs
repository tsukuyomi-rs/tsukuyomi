//! Components for constructing and handling HTTP errors.

pub mod handler;

use failure::{self, Fail};
use http::header::HeaderMap;
use http::StatusCode;
use std::error;

/// A type alias representing a critical error.
pub type CritError = Box<error::Error + Send + Sync + 'static>;

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = ::std::result::Result<T, Error>;

/// [unstable]
/// A trait representing HTTP errors.
pub trait HttpError: Fail {
    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode;

    /// Appends some entries into the header map of an HTTP response.
    #[allow(unused_variables)]
    fn append_headers(&self, h: &mut HeaderMap) {}
}

/// A type which holds all kinds of errors occurring in handlers.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    Boxed(Box<HttpError>),
    Concrete(ConcreteHttpError),
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
    pub fn from_failure<E>(cause: E, status: StatusCode) -> Error
    where
        E: Into<failure::Error>,
    {
        Error {
            kind: ErrorKind::Concrete(ConcreteHttpError {
                cause: cause.into(),
                status: status,
            }),
        }
    }

    /// Creates an HTTP error representing "404 Not Found".
    pub fn not_found() -> Error {
        Error::from_failure(format_err!("Not Found"), StatusCode::NOT_FOUND)
    }

    /// Creates an HTTP error representing "405 Method Not Allowed".
    pub fn method_not_allowed() -> Error {
        Error::from_failure(format_err!("Method Not Allowed"), StatusCode::METHOD_NOT_ALLOWED)
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

    /// Returns the representation as `HttpError` of this error value.
    ///
    /// If the value is a criticial error, it will return a `None`.
    pub fn as_http_error(&self) -> Option<&HttpError> {
        match self.kind {
            ErrorKind::Concrete(ref e) => Some(e),
            ErrorKind::Boxed(ref e) => Some(&**e),
            ErrorKind::Crit(..) => None,
        }
    }

    pub(crate) fn into_critical(self) -> Option<CritError> {
        match self.kind {
            ErrorKind::Crit(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Fail)]
#[fail(display = "{}", cause)]
struct ConcreteHttpError {
    cause: failure::Error,
    status: StatusCode,
}

impl HttpError for ConcreteHttpError {
    fn status_code(&self) -> StatusCode {
        self.status
    }
}
