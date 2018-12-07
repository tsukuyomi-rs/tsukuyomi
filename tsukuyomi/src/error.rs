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

use {
    crate::{
        core::Never,
        output::{Output, ResponseBody},
    },
    http::{Request, Response, StatusCode},
    std::{
        any::TypeId,
        fmt, io,
        ops::{Deref, DerefMut},
    },
};

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A trait representing error values to be converted into an HTTP response.
pub trait HttpError: fmt::Display + fmt::Debug + Send + 'static {
    /// Returns an HTTP status code associated with this value.
    fn status_code(&self) -> StatusCode;

    /// Generates a message body from this error value.
    #[allow(unused_variables)]
    fn to_response(&mut self, request: &Request<()>) -> Output {
        Response::builder()
            .status(self.status_code())
            .body(self.to_string().into())
            .expect("should be a valid response")
    }

    #[doc(hidden)]
    fn __private_type_id__(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn HttpError {
    /// Returns `true` if the type of inner error value is equal to `T`.
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

    /// Attempts to downcast this error value to the specified concrete type by reference.
    pub fn downcast_mut<T: HttpError>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe { Some(&mut *(self as *mut dyn HttpError as *mut T)) }
        } else {
            None
        }
    }
}

impl HttpError for StatusCode {
    fn status_code(&self) -> StatusCode {
        *self
    }

    fn to_response(&mut self, _: &Request<()>) -> Output {
        let mut response = Response::new(ResponseBody::default());
        *response.status_mut() = *self;
        response
    }
}

/// The implementation of `HttpError` for the standard I/O error.
impl HttpError for io::Error {
    fn to_response(&mut self, _: &Request<()>) -> Output {
        Response::builder()
            .status(self.status_code())
            .body(format!("I/O error: {}", self).into())
            .expect("should be a valid response")
    }

    fn status_code(&self) -> StatusCode {
        match self.kind() {
            io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// The implementation of `HttpError` for the generic error provided by `failure`.
impl HttpError for failure::Error {
    fn to_response(&mut self, _: &Request<()>) -> Output {
        Response::builder()
            .status(self.status_code())
            .body(format!("generic error: {}", self).into())
            .expect("should be a valid response")
    }

    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl HttpError for hyper::Error {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn to_response(&mut self, _: &Request<()>) -> Output {
        Response::builder()
            .status(self.status_code())
            .body(format!("hyper error: {}", self).into())
            .expect("should be a valid response")
    }
}

impl HttpError for Never {
    fn status_code(&self) -> StatusCode {
        match *self {}
    }

    fn to_response(&mut self, _: &Request<()>) -> Output {
        match *self {}
    }
}

/// An error type which wraps a `Display`able value.
#[derive(Debug)]
pub struct Custom<D> {
    parts: Option<http::response::Parts>,
    body: D,
}

#[allow(missing_docs)]
impl<D> Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    pub fn new(response: Response<D>) -> Self {
        debug_assert!(response.status().is_client_error() || response.status().is_server_error());
        let (parts, body) = response.into_parts();
        Self {
            parts: Some(parts),
            body,
        }
    }

    pub fn parts(&mut self) -> &mut http::response::Parts {
        self.parts
            .as_mut()
            .expect("The error has already converted into response")
    }

    pub fn map<F, U>(self, f: F) -> Custom<U>
    where
        F: FnOnce(D) -> U,
    {
        Custom {
            parts: self.parts,
            body: f(self.body),
        }
    }

    pub fn into_body(self) -> D {
        self.body
    }
}

impl<D> Deref for Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    type Target = D;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

impl<D> DerefMut for Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.body
    }
}

impl<D> From<Response<D>> for Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    fn from(response: Response<D>) -> Self {
        Self::new(response)
    }
}

impl<D> fmt::Display for Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.body, f)
    }
}

impl<D> HttpError for Custom<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    fn status_code(&self) -> StatusCode {
        match self.parts {
            Some(ref parts) => parts.status,
            None => panic!(""),
        }
    }

    fn to_response(&mut self, _: &Request<()>) -> Output {
        let parts = self
            .parts
            .take()
            .expect("The error has already converted into response");
        let body = self.body.to_string();
        Response::from_parts(parts, body.into())
    }
}

#[allow(missing_docs)]
pub fn custom<D>(status: StatusCode, msg: D) -> Error
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    Custom::new({
        let mut response = Response::new(msg);
        *response.status_mut() = status;
        response
    })
    .into()
}

#[allow(missing_docs)]
pub fn response<D>(response: Response<D>) -> Error
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    Custom::new(response).into()
}

macro_rules! define_errors {
    ($(
        $(#[$m:meta])*
        $name:ident => $STATUS:ident,
    )*) => {$(
        $(#[$m])*
        #[inline]
        pub fn $name<D>(msg: D) -> Error
        where
            D: fmt::Debug + fmt::Display + Send + 'static,
        {
            self::custom(StatusCode::$STATUS, msg)
        }
    )*};
}

define_errors! {
    /// Equivalent to `custom(StatusCode::BAD_REQUEST, msg)`.
    bad_request => BAD_REQUEST,

    /// Equivalent to `custom(StatusCode::UNAUTHORIZED, msg)`.
    unauthorized => UNAUTHORIZED,

    /// Equivalent to `custom(StatusCode::FORBIDDEN, msg)`.
    forbidden => FORBIDDEN,

    /// Equivalent to `custom(StatusCode::NOT_FOUND, msg)`.
    not_found => NOT_FOUND,

    /// Equivalent to `custom(StatusCode::METHOD_NOT_ALLOWED, msg)`.
    method_not_allowed => METHOD_NOT_ALLOWED,

    /// Equivalent to `custom(StatusCode::INTERNAL_SERVER_ERROR, msg)`.
    internal_server_error => INTERNAL_SERVER_ERROR,
}

// ==== Error ====

/// A type which holds all kinds of errors occurring in handlers.
#[derive(Debug)]
pub struct Error(Box<dyn HttpError>);

impl<E> From<E> for Error
where
    E: HttpError,
{
    fn from(err: E) -> Self {
        Error(Box::new(err))
    }
}

impl Deref for Error {
    type Target = dyn HttpError;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Error {
    /// Deconstructs `self` into inner error representation.
    pub fn into_http_error(self) -> Box<dyn HttpError> {
        self.0
    }

    /// Attempts to downcast this error value into the specified concrete type.
    pub fn downcast<T: HttpError>(self) -> std::result::Result<T, Self> {
        if self.0.__private_type_id__() == TypeId::of::<T>() {
            unsafe { Ok(*Box::from_raw(Box::into_raw(self.0) as *mut T)) }
        } else {
            Err(self)
        }
    }

    pub(crate) fn into_response(self, request: &Request<()>) -> Output {
        let mut err = self.0;
        let status = err.status_code();
        let mut response = err.to_response(request);
        *response.status_mut() = status;
        response
    }
}
