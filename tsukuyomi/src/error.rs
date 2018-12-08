//! Error representation during handling the request.
//!
//! Tsukuyomi models the all errors generated during handling HTTP requests with a trait
//! named [`HttpError`]. This trait is a method for converting itself into an HTTP response.
//! The design of this trait imitates the `failure` crate, but there are some specialization
//! considering of the HTTP context.
//!
//! [`HttpError`]: ./trait.HttpError.html

use {
    crate::{
        core::Never,
        output::{Output, ResponseBody},
    },
    http::{Request, Response, StatusCode},
    std::{any::TypeId, fmt, io},
};

/// A type alias of `Result<T, E>` with `error::Error` as error type.
pub type Result<T> = std::result::Result<T, Error>;

/// A trait representing error values to be converted into an HTTP response.
///
/// The role of this trait is similar to `Responder`, but there are the following
/// differences:
///
/// * `HttpError` cannot access the entire of request context.
/// * `HttpError::into_response` is infallible.
/// * The error values are stored as an object, and the conversion into
///   HTTP responses will be deferred until just before replying to the
///   client.
pub trait HttpError: fmt::Display + fmt::Debug + Send + 'static + Sized {
    type Body: Into<ResponseBody>;

    /// Consumes itself and creates an HTTP response from its value.
    fn into_response(self, request: &Request<()>) -> Response<Self::Body>;
}

impl HttpError for StatusCode {
    type Body = ();

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        let mut response = Response::new(());
        *response.status_mut() = self;
        response
    }
}

/// The implementation of `HttpError` for the standard I/O error.
impl HttpError for io::Error {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        Response::builder()
            .status(match self.kind() {
                io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            })
            .body(format!("I/O error: {}", self))
            .expect("should be a valid response")
    }
}

/// The implementation of `HttpError` for the generic error provided by `failure`.
impl HttpError for failure::Error {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("generic error: {}", self))
            .expect("should be a valid response")
    }
}

impl HttpError for hyper::Error {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("hyper error: {}", self))
            .expect("should be a valid response")
    }
}

impl HttpError for Never {
    type Body = ResponseBody;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        match self {}
    }
}

/// An error type which wraps a `Display`able value.
#[derive(Debug)]
pub struct ErrorResponse<T> {
    inner: Response<T>,
}

#[allow(missing_docs)]
impl<T> ErrorResponse<T>
where
    T: fmt::Debug + fmt::Display + Send + 'static,
{
    pub fn new(inner: Response<T>) -> Self {
        debug_assert!(inner.status().is_client_error() || inner.status().is_server_error());
        Self { inner }
    }

    pub fn into_inner(self) -> Response<T> {
        self.inner
    }
}

impl<D> From<Response<D>> for ErrorResponse<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    fn from(response: Response<D>) -> Self {
        Self::new(response)
    }
}

impl<D> fmt::Display for ErrorResponse<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.inner.body(), f)
    }
}

impl<D> HttpError for ErrorResponse<D>
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        self.inner.map(|body| body.to_string())
    }
}

#[allow(missing_docs)]
pub fn error_response<D>(response: Response<D>) -> Error
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    ErrorResponse::new(response).into()
}

#[allow(missing_docs)]
pub fn custom<D>(status: StatusCode, msg: D) -> Error
where
    D: fmt::Debug + fmt::Display + Send + 'static,
{
    let mut response = Response::new(msg);
    *response.status_mut() = status;
    error_response(response)
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

/// A custom trait object which holds all kinds of errors occurring in handlers.
pub struct Error {
    obj: AnyObj,
    fmt_debug_fn: unsafe fn(&AnyObj, &mut fmt::Formatter<'_>) -> fmt::Result,
    fmt_display_fn: unsafe fn(&AnyObj, &mut fmt::Formatter<'_>) -> fmt::Result,
    into_response_fn: unsafe fn(AnyObj, &Request<()>) -> Output,
}

impl fmt::Debug for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (self.fmt_debug_fn)(&self.obj, formatter) }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (self.fmt_display_fn)(&self.obj, formatter) }
    }
}

impl<E> From<E> for Error
where
    E: HttpError,
{
    fn from(err: E) -> Self {
        Self::new(err)
    }
}

impl Error {
    pub fn new<E: HttpError>(err: E) -> Self {
        unsafe fn fmt_debug<E: HttpError>(
            this: &AnyObj,
            f: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            debug_assert!(this.is::<E>());
            fmt::Debug::fmt(this.downcast_ref_unchecked::<E>(), f)
        }

        unsafe fn fmt_display<E: HttpError>(
            this: &AnyObj,
            f: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            debug_assert!(this.is::<E>());
            fmt::Display::fmt(this.downcast_ref_unchecked::<E>(), f)
        }

        unsafe fn into_response<E: HttpError>(this: AnyObj, request: &Request<()>) -> Output {
            debug_assert!(this.is::<E>());
            HttpError::into_response(this.downcast_unchecked::<E>(), request).map(Into::into)
        }

        Error {
            obj: AnyObj::new(err),
            fmt_debug_fn: fmt_debug::<E>,
            fmt_display_fn: fmt_display::<E>,
            into_response_fn: into_response::<E>,
        }
    }

    /// Returns `true` if the inner error value has the type of `T`.
    #[inline]
    pub fn is<T: HttpError>(&self) -> bool {
        self.obj.is::<T>()
    }

    /// Attempts to downcast this error value to the specified concrete type by reference.
    #[inline]
    pub fn downcast_ref<T: HttpError>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe { Some(self.obj.downcast_ref_unchecked()) }
        } else {
            None
        }
    }

    /// Attempts to downcast this error value to the specified concrete type by reference.
    #[inline]
    pub fn downcast_mut<T: HttpError>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe { Some(self.obj.downcast_mut_unchecked()) }
        } else {
            None
        }
    }

    /// Attempts to downcast this error value into the specified concrete type.
    #[inline]
    pub fn downcast<T: HttpError>(self) -> std::result::Result<T, Self> {
        if self.is::<T>() {
            unsafe { Ok(self.obj.downcast_unchecked()) }
        } else {
            Err(self)
        }
    }

    /// Consumes itself and creates an HTTP response from its value.
    pub fn into_response(self, request: &Request<()>) -> Output {
        unsafe { (self.into_response_fn)(self.obj, request) }
    }
}

#[allow(missing_debug_implementations)]
struct AnyObj {
    ptr: *mut (),
    type_id: TypeId,
    drop_fn: unsafe fn(*mut ()),
}

impl Drop for AnyObj {
    fn drop(&mut self) {
        unsafe { (self.drop_fn)(self.ptr) }
    }
}

impl AnyObj {
    fn new<T: 'static>(value: T) -> Self {
        unsafe fn unsafe_drop<T>(ptr: *mut ()) {
            std::mem::drop(Box::from_raw(ptr as *mut T))
        }

        AnyObj {
            ptr: Box::into_raw(Box::new(value)) as *mut (),
            type_id: TypeId::of::<T>(),
            drop_fn: unsafe_drop::<T>,
        }
    }

    #[inline]
    fn is<T: 'static>(&self) -> bool {
        self.type_id == TypeId::of::<T>()
    }

    #[inline]
    unsafe fn downcast_ref_unchecked<T: 'static>(&self) -> &T {
        &*(self.ptr as *const T)
    }

    #[inline]
    unsafe fn downcast_mut_unchecked<T: 'static>(&mut self) -> &mut T {
        &mut *(self.ptr as *mut T)
    }

    #[inline]
    unsafe fn downcast_unchecked<T: 'static>(self) -> T {
        let value = *Box::from_raw(self.ptr as *mut T);
        std::mem::forget(self); // cancel AnyObj::drop
        value
    }
}
