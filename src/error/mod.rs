//! Components for constructing and handling HTTP errors.

pub mod handler;

use failure::{self, Fail};
use http::{Request, Response, StatusCode};
use std::any::TypeId;
use std::{error, fmt};

use input::RequestBody;
use output::ResponseBody;

/// A type alias representing a critical error.
pub type CritError = Box<dyn error::Error + Send + Sync + 'static>;

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = ::std::result::Result<T, Error>;

/// A trait representing error types to be converted into HTTP response.
pub trait HttpError: fmt::Debug + fmt::Display + Send + 'static {
    /// Convert this error value into an HTTP response.
    #[allow(unused_variables)]
    fn into_response(
        &mut self,
        request: &Request<RequestBody>,
    ) -> Option<::std::result::Result<Response<ResponseBody>, CritError>> {
        None
    }

    //

    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Returns the representation as a `Fail`, if possible.
    fn as_fail(&self) -> Option<&dyn Fail> {
        None
    }

    #[doc(hidden)]
    fn __private_type_id__(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn HttpError {
    #[allow(missing_docs)]
    #[inline(always)]
    pub fn is<T: HttpError>(&self) -> bool {
        self.__private_type_id__() == TypeId::of::<T>()
    }

    /// Attempts to downcast this error value to the specified concrete type by reference.
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe { Some(&*(self as *const dyn HttpError as *const T)) }
        } else {
            None
        }
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
        Error::new(err)
    }
}

impl Error {
    /// Creates an HTTP error from an error value and an HTTP status code.
    pub fn new(err: impl HttpError) -> Error {
        Error {
            kind: ErrorKind::Boxed(Box::new(err)),
        }
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

    /// Returns the representation as `HttpError` of this error value by reference.
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

    #[allow(missing_docs)]
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        self.as_http_error()?.downcast_ref()
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

    /// Creates an HTTP error representing "404 Not Found".
    pub fn not_found() -> Failure {
        Failure::new(StatusCode::NOT_FOUND, format_err!("Not Found"))
    }

    /// Creates an HTTP error representing "405 Method Not Allowed".
    pub fn method_not_allowed() -> Failure {
        Failure::new(
            StatusCode::METHOD_NOT_ALLOWED,
            format_err!("Method Not Allowed"),
        )
    }

    /// Creates an HTTP error representing "400 Bad Request" from the provided error value.
    pub fn bad_request(err: impl Into<failure::Error>) -> Failure {
        Failure::new(StatusCode::BAD_REQUEST, err)
    }

    /// Creates an HTTP error representing "500 Internal Server Error", from the provided error value .
    pub fn internal_server_error(err: impl Into<failure::Error>) -> Failure {
        Failure::new(StatusCode::INTERNAL_SERVER_ERROR, err)
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
