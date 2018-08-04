//! The definition of error handlers.

use super::Error;

/// A trait representing a global error handlers.
pub trait ErrorHandler {
    /// Modifies a specified error value
    ///
    /// This method will be called before converting the value of `Error`
    /// into an HTTP response.
    fn handle_error(&self, err: Error) -> Error;
}

impl<F> ErrorHandler for F
where
    F: Fn(Error) -> Error,
{
    fn handle_error(&self, err: Error) -> Error {
        (*self)(err)
    }
}

/// An implementor of `ErrorHandler` used in `App` by default.
#[derive(Debug, Default)]
pub(crate) struct DefaultErrorHandler {
    _priv: (),
}

impl ErrorHandler for DefaultErrorHandler {
    fn handle_error(&self, err: Error) -> Error {
        err
    }
}
