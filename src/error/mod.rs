pub mod handler;

use failure::{self, Fail};
use http::StatusCode;
use std::{error, fmt};

pub type CritError = Box<error::Error + Send + Sync + 'static>;

pub trait HttpError: fmt::Debug + fmt::Display + Send + Sync + 'static {
    fn status_code(&self) -> StatusCode;
}

impl<E> HttpError for E
where
    E: Fail,
{
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

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
        Error::new(err)
    }
}

impl Error {
    pub fn new<E>(err: E) -> Error
    where
        E: HttpError,
    {
        Error {
            kind: ErrorKind::Boxed(Box::new(err)),
        }
    }

    /// Constructs an HTTP error from components.
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

    pub fn not_found() -> Error {
        Error::from_failure(format_err!("Not Found"), StatusCode::NOT_FOUND)
    }

    pub fn method_not_allowed() -> Error {
        Error::from_failure(format_err!("Invalid Method"), StatusCode::METHOD_NOT_ALLOWED)
    }

    pub fn bad_request<E>(e: E) -> Error
    where
        E: Into<failure::Error>,
    {
        Error::from_failure(e, StatusCode::BAD_REQUEST)
    }

    pub fn internal_server_error<E>(e: E) -> Error
    where
        E: Into<failure::Error>,
    {
        Error::from_failure(e, StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// Constructs a *critical* error from a value.
    ///
    /// The word *critical* means that the error does not converted to an HTTP response and will be
    /// passed directly to the lower-level HTTP service.
    pub fn critical<E>(err: E) -> Error
    where
        E: Into<Box<error::Error + Send + Sync + 'static>>,
    {
        Error {
            kind: ErrorKind::Crit(err.into()),
        }
    }

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

#[derive(Debug)]
struct ConcreteHttpError {
    cause: failure::Error,
    status: StatusCode,
}

impl fmt::Display for ConcreteHttpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.cause, f)
    }
}

impl HttpError for ConcreteHttpError {
    fn status_code(&self) -> StatusCode {
        self.status
    }
}
