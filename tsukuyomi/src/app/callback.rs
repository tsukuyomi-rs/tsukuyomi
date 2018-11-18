use http::Request;

use crate::error::{Critical, Error};
use crate::output::Output;

/// A trait representing a global error handlers.
pub trait Callback: Send + Sync + 'static {
    /// Converts an error value into an HTTP response.
    fn on_error(&self, err: Error, request: &Request<()>) -> Result<Output, Critical> {
        err.into_response(request)
    }
}

impl Callback for () {}
