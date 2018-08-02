//! The definition of error handlers.

use http::{header, Request, Response};

use output::ResponseBody;

use super::{CritError, HttpError};

/// A trait representing error handlers.
pub trait ErrorHandler {
    /// Creates an HTTP response from the provided error value.
    fn handle_error(
        &self,
        err: &dyn HttpError,
        request: &Request<()>,
    ) -> Result<Response<ResponseBody>, CritError>;
}

impl<F, T> ErrorHandler for F
where
    F: Fn(&dyn HttpError, &Request<()>) -> Result<Response<T>, CritError>,
    T: Into<ResponseBody>,
{
    fn handle_error(
        &self,
        err: &dyn HttpError,
        request: &Request<()>,
    ) -> Result<Response<ResponseBody>, CritError> {
        (*self)(err, request).map(|res| res.map(Into::into))
    }
}

/// An implementor of `ErrorHandler` used in `App` by default.
#[derive(Debug, Default)]
pub struct DefaultErrorHandler {
    _priv: (),
}

impl DefaultErrorHandler {
    /// Creates a new instance of `DefaultErrorHandler`.
    pub fn new() -> DefaultErrorHandler {
        Default::default()
    }
}

impl ErrorHandler for DefaultErrorHandler {
    fn handle_error(
        &self,
        err: &dyn HttpError,
        _: &Request<()>,
    ) -> Result<Response<ResponseBody>, CritError> {
        Response::builder()
            .status(err.status_code())
            .header(header::CONNECTION, "close")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(err.to_string().into())
            .map_err(|e| {
                format_err!("failed to construct an HTTP error response: {}", e)
                    .compat()
                    .into()
            })
    }
}
