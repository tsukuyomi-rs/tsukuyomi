//! Error representation during handling the request.
//!
//! Tsukuyomi models the all errors generated during handling HTTP requests with a trait
//! named [`HttpError`]. This trait is a method for converting itself into an HTTP response.
//!
//! [`HttpError`]: ./trait.HttpError.html

use {
    crate::{
        output::{IntoResponse, ResponseBody},
        util::Never,
    },
    failure::{AsFail, Fail},
    http::{Request, Response, StatusCode},
    std::{any::TypeId, fmt, io},
};

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A trait representing error values to be converted into an HTTP response.
///
/// Roughly speaking, this trait extends `failure::Fail`, a nearly standard of
/// error abstraction in Rust, and provides the additional properties for handling
/// the value as an HTTP response.
///
/// Note that this trait is defined as a sub trait of `AsFail` (not `Fail` itself),
/// due to some restrictions of the current trait system.
pub trait HttpError: AsFail + fmt::Debug + fmt::Display + Send + Sync + 'static {
    /// Returns an HTTP status code associated with this error value.
    fn status_code(&self) -> StatusCode;

    /// Creates an HTTP response from this error value.
    fn to_response(&self) -> Response<()> {
        let mut response = Response::new(());
        *response.status_mut() = self.status_code();
        response
    }

    // not a public API.
    #[doc(hidden)]
    fn __private_type_id__(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn HttpError {
    /// Returns whether the concrete type of this object is the same as `T` or not.
    pub fn is<T: HttpError>(&self) -> bool {
        self.__private_type_id__() == TypeId::of::<T>()
    }

    /// Attempts to downcast this object into the specified concrete type as a reference.
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe { Some(&*(self as *const Self as *const T)) }
        } else {
            None
        }
    }

    /// Attempts to downcast this object into the specified concrete type as a mutable reference.
    pub fn downcast_mut<T: HttpError>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe { Some(&mut *(self as *mut Self as *mut T)) }
        } else {
            None
        }
    }

    /// Attempts to downcast this object into the specified concrete type without unboxing.
    pub fn downcast<T: HttpError>(self: Box<Self>) -> std::result::Result<Box<T>, Box<Self>> {
        if self.is::<T>() {
            unsafe { Ok(Box::from_raw(Box::into_raw(self) as *mut T)) }
        } else {
            Err(self)
        }
    }
}

/// The implementation of `HttpError` for the standard I/O error.
impl HttpError for io::Error {
    fn status_code(&self) -> StatusCode {
        match self.kind() {
            io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl HttpError for failure::Error {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl HttpError for Never {
    fn status_code(&self) -> StatusCode {
        match *self {}
    }
}

/// Creates an `Error` from the specified message and HTTP status code.
pub fn err_msg<D>(status: StatusCode, msg: D) -> Error
where
    D: fmt::Debug + fmt::Display + Send + Sync + 'static,
{
    #[derive(Debug, failure::Fail)]
    #[fail(display = "{}", msg)]
    struct ErrorMessage<D>
    where
        D: fmt::Debug + fmt::Display + Send + Sync + 'static,
    {
        status: StatusCode,
        msg: D,
    }

    impl<D> HttpError for ErrorMessage<D>
    where
        D: fmt::Debug + fmt::Display + Send + Sync + 'static,
    {
        fn status_code(&self) -> StatusCode {
            self.status
        }
    }

    ErrorMessage { status, msg }.into()
}

macro_rules! define_custom_errors {
    ($(
        $(#[$m:meta])*
        $name:ident => $STATUS:ident,
    )*) => {$(
        $(#[$m])*
        #[inline]
        pub fn $name<D>(msg: D) -> Error
        where
            D: fmt::Debug + fmt::Display + Send + Sync + 'static,
        {
            self::err_msg(StatusCode::$STATUS, msg)
        }
    )*};
}

define_custom_errors! {
    /// Equivalent to `err_msg(StatusCode::BAD_REQUEST, msg)`.
    bad_request => BAD_REQUEST,

    /// Equivalent to `err_msg(StatusCode::UNAUTHORIZED, msg)`.
    unauthorized => UNAUTHORIZED,

    /// Equivalent to `err_msg(StatusCode::FORBIDDEN, msg)`.
    forbidden => FORBIDDEN,

    /// Equivalent to `err_msg(StatusCode::NOT_FOUND, msg)`.
    not_found => NOT_FOUND,

    /// Equivalent to `err_msg(StatusCode::METHOD_NOT_ALLOWED, msg)`.
    method_not_allowed => METHOD_NOT_ALLOWED,

    /// Equivalent to `err_msg(StatusCode::INTERNAL_SERVER_ERROR, msg)`.
    internal_server_error => INTERNAL_SERVER_ERROR,
}

type DynStdError = dyn std::error::Error + Send + Sync + 'static;

/// A wrapper type for treating arbitrary error type as an `HttpError`.
///
/// The value of this type are built at constructing `Error` from
/// `Box<dyn Error + Send + Sync>`.
#[derive(Debug, failure::Fail)]
#[fail(display = "{}", _0)]
pub struct BoxedStdCompat(Box<DynStdError>);

impl BoxedStdCompat {
    pub fn into_inner(self) -> Box<DynStdError> {
        self.0
    }

    pub fn get_ref(&self) -> &DynStdError {
        &*self.0
    }

    pub fn downcast<T>(self) -> std::result::Result<T, Self>
    where
        T: std::error::Error + Send + Sync + 'static,
    {
        self.0.downcast::<T>().map(|t| *t).map_err(BoxedStdCompat)
    }

    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: std::error::Error + Send + Sync + 'static,
    {
        self.0.downcast_ref::<T>()
    }
}

impl HttpError for BoxedStdCompat {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

// ==== Error ====

/// A type that contains arbitrary HTTP error values.
#[derive(Debug)]
pub struct Error {
    inner: Box<dyn HttpError>,
}

impl<E> From<E> for Error
where
    E: HttpError,
{
    fn from(err: E) -> Self {
        Self {
            inner: Box::new(err),
        }
    }
}

impl From<StatusCode> for Error {
    fn from(status: StatusCode) -> Self {
        #[derive(Debug, failure::Fail)]
        #[fail(display = "{}", _0)]
        struct CompatStatus(StatusCode);

        impl HttpError for CompatStatus {
            fn status_code(&self) -> StatusCode {
                self.0
            }
        }

        Self {
            inner: Box::new(CompatStatus(status)),
        }
    }
}

impl From<Box<DynStdError>> for Error {
    fn from(e: Box<DynStdError>) -> Self {
        Self {
            inner: Box::new(BoxedStdCompat(e)),
        }
    }
}

impl Error {
    /// Returns an HTTP status code associated with the underlying error value.
    pub fn status_code(&self) -> StatusCode {
        self.inner.status_code()
    }

    /// Creates an HTTP response from the underlying error value.
    pub fn to_response(&self) -> Response<()> {
        self.inner.to_response()
    }

    pub(crate) fn into_response(self) -> crate::output::Response {
        let (mut parts, ()) = self.inner.to_response().into_parts();
        parts.extensions.insert(self);
        Response::from_parts(parts, ResponseBody::empty())
    }

    /// Attempts to downcast the underlying error value into the specified concrete type.
    pub fn downcast<T: HttpError>(self) -> Result<T> {
        match self.inner.downcast::<T>() {
            Ok(t) => Ok(*t),
            Err(inner) => Err(Self { inner }),
        }
    }

    /// Attempts to downcast the underlying error value into the specified concrete type as a reference.
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        self.inner.downcast_ref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.inner, formatter)
    }
}

impl AsFail for Error {
    fn as_fail(&self) -> &dyn Fail {
        self.inner.as_fail()
    }
}

impl IntoResponse for Error {
    fn into_response(self, _: &Request<()>) -> Result<crate::output::Response> {
        Ok(self.into_response())
    }
}
