//! The definition of error handlers.

use http::{Request, Response};

use super::Error;

use crate::input::RequestBody;
use crate::output::ResponseBody;
use crate::runtime::CritError;

/// A trait representing a global error handlers.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ErrorHandler {
    /// Converts an error value into an HTTP response.
    fn handle_error(
        &self,
        err: Error,
        request: &Request<RequestBody>,
    ) -> Result<Response<ResponseBody>, CritError>;
}

impl<F, Bd> ErrorHandler for F
where
    F: Fn(Error, &Request<RequestBody>) -> Result<Response<Bd>, CritError>,
    Bd: Into<ResponseBody>,
{
    fn handle_error(
        &self,
        err: Error,
        request: &Request<RequestBody>,
    ) -> Result<Response<ResponseBody>, CritError> {
        (*self)(err, request).map(|response| response.map(Into::into))
    }
}

/// An implementor of `ErrorHandler` used in `App` by default.
#[derive(Debug, Default)]
pub(crate) struct DefaultErrorHandler {
    _priv: (),
}

impl ErrorHandler for DefaultErrorHandler {
    fn handle_error(
        &self,
        err: Error,
        request: &Request<RequestBody>,
    ) -> Result<Response<ResponseBody>, CritError> {
        let mut err = err.0?;
        let status = err.status_code();
        let mut response = err.to_response(request);
        *response.status_mut() = status;
        Ok(response)
    }
}
