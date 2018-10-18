//! Components for constructing and handling HTTP errors.
//!
//! # Error Representation
//!
//! Tsukuyomi models the all errors generated during handling HTTP requests with a trait
//! named [`HttpError`]. This trait is a sub trait of [`Fail`] with additional methods for
//! converting itself to an HTTP response.
//!
//! The all of handling errors are managed in the framework by converting into an [`Error`].
//! They will be automatically converted to an HTTP response after all processeing will be
//! completed.
//!
//! [`Error`]: ./struct.Error.html
//! [`Fail`]: https://docs.rs/failure/0.1/failure/trait.Fail.html
//! [`HttpError`]: ./trait.HttpError.html
//!
//! # Error Handling
//!
//! The best way to specify the error responses is usually to return a value which implements
//! `HttpError`. However, The error values after being converted to `Error` can be modified
//! by using the following components:
//!
//! * [`ErrorHandler`] - It modifies the all kinds of errors which occurred during handling.
//! * [`Modifier`] - It modifies errors occurred within a certain scope.
//!
//! [`Modifier`]: ../modifier/trait.Modifier.html
//! [`ErrorHandler`]: ./trait.ErrorHandler.html

mod failure;
mod handler;
mod message;
mod never;

pub use self::failure::Failure;
pub(crate) use self::handler::DefaultErrorHandler;
pub use self::handler::ErrorHandler;
pub use self::message::{bad_request, internal_server_error, ErrorMessage};
pub use self::never::Never;

// ====

use http::header::HeaderMap;
use http::{header, Request, Response, StatusCode};
use std::any::TypeId;
use std::io;

use crate::input::RequestBody;
use crate::output::ResponseBody;

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = ::std::result::Result<T, Error>;

/// A trait representing error values to be converted into an HTTP response.
pub trait HttpError: ::failure::Fail {
    /// Returns an HTTP status code associated with this value.
    ///
    /// By default, the value is `500 Internal Server Error`.
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Appends some entries to the provided header map.
    #[allow(unused_variables)]
    fn headers(&self, headers: &mut HeaderMap) {}

    /// Generates a message body from this error value.
    ///
    /// By default, it just uses `fmt::Display`.
    #[allow(unused_variables)]
    fn body(&mut self, request: &Request<RequestBody>) -> ResponseBody {
        self.to_string().into()
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

    /// Attempts to downcast this error value to the specified concrete type by shared reference.
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe { Some(&*(self as *const dyn HttpError as *const T)) }
        } else {
            None
        }
    }

    /// Attempts to downcast this error value to the specified concrete type by mutable reference.
    pub fn downcast_mut<T: HttpError>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe { Some(&mut *(self as *mut dyn HttpError as *mut T)) }
        } else {
            None
        }
    }
}

/// An extension trait for adding support for downcasting of `HttpError`s.
pub trait BoxHttpErrorExt {
    /// Attempts to downcast the error value to the specified concreate type.
    fn downcast<T: HttpError>(self) -> ::std::result::Result<Box<T>, Box<dyn HttpError>>;
}

impl BoxHttpErrorExt for Box<dyn HttpError> {
    fn downcast<T: HttpError>(self) -> ::std::result::Result<Box<T>, Box<dyn HttpError>> {
        if self.is::<T>() {
            unsafe { Ok(Box::from_raw(Box::into_raw(self) as *mut T)) }
        } else {
            Err(self)
        }
    }
}

/// The implementation of HttpError for the standard I/O error.
impl HttpError for io::Error {
    fn status(&self) -> StatusCode {
        match self.kind() {
            io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// A type which holds all kinds of errors occurring in handlers.
#[derive(Debug)]
pub struct Error(::std::result::Result<Box<dyn HttpError>, crate::server::CritError>);

impl<E> From<E> for Error
where
    E: HttpError,
{
    fn from(err: E) -> Error {
        Error::new(Box::new(err) as Box<dyn HttpError>)
    }
}

impl Error {
    /// Creates an `Error` from the specified value implementing `HttpError`.
    pub fn new(err: impl Into<Box<dyn HttpError>>) -> Error {
        Error(Ok(err.into()))
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
        E: Into<crate::server::CritError>,
    {
        Error(Err(err.into()))
    }

    /// Returns `true` if this error is a *critical* error.
    pub fn is_critical(&self) -> bool {
        self.0.is_err()
    }

    /// Returns the representation as `HttpError` of this error value by reference.
    ///
    /// If the value is a criticial error, it will return a `None`.
    pub fn as_http_error(&self) -> Option<&dyn HttpError> {
        match self.0 {
            Ok(ref e) => Some(&**e),
            Err(..) => None,
        }
    }

    /// Consumes `self` and converts its value into a boxed `HttpError`.
    ///
    /// If the value is a criticial error, it returns `self` wrapped in `Err`.
    pub fn into_http_error(self) -> ::std::result::Result<Box<dyn HttpError>, Error> {
        match self.0 {
            Ok(e) => Ok(e),
            Err(e) => Err(Error(Err(e))),
        }
    }

    /// Attempts to downcast this error value into the specified concrete type.
    pub fn downcast<T: HttpError>(self) -> ::std::result::Result<T, Error> {
        match self.0 {
            Ok(e) => e.downcast().map(|e| *e).map_err(|e| Error(Ok(e))),
            Err(e) => Err(Error(Err(e))),
        }
    }

    /// Attempts to downcast this error value to the specified concrete type by reference.
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        match self.0 {
            Ok(ref e) => e.downcast_ref(),
            Err(..) => None,
        }
    }

    /// Attempts to downcast this error value to the specified concrete type by reference.
    pub fn downcast_mut<T: HttpError>(&mut self) -> Option<&mut T> {
        match self.0 {
            Ok(ref mut e) => e.downcast_mut(),
            Err(..) => None,
        }
    }

    /// [unstable]
    /// Attempts to convert the internal value of `HttpError` into a specified type.
    ///
    /// This method does nothing if the type id of internal value is equal to `T`.
    pub fn map<T: HttpError>(self, f: impl FnOnce(Box<dyn HttpError>) -> T) -> Error {
        Error(match self.0 {
            Ok(e) => match e.downcast::<T>() {
                Ok(e) => Ok(e),
                Err(e) => Ok(Box::new(f(e))),
            },
            Err(e) => Err(e),
        })
    }

    pub(crate) fn into_response(
        self,
        request: &Request<RequestBody>,
    ) -> ::std::result::Result<Response<ResponseBody>, crate::server::CritError> {
        let mut err = self.0?;
        let mut response = Response::builder()
            .status(err.status())
            .header(header::CONNECTION, "close")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(())?;
        err.headers(response.headers_mut());
        Ok(response.map(|()| err.body(request)))
    }
}
