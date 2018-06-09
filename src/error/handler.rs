use http::{Request, Response};

use output::ResponseBody;

use super::{CritError, Error};

pub trait ErrorHandler {
    fn handle_error(&self, err: Error, request: &Request<()>) -> Result<Response<ResponseBody>, CritError>;
}

impl<F, T> ErrorHandler for F
where
    F: Fn(Error, &Request<()>) -> Result<Response<T>, CritError>,
    T: Into<ResponseBody>,
{
    fn handle_error(&self, err: Error, request: &Request<()>) -> Result<Response<ResponseBody>, CritError> {
        (*self)(err, request).map(|res| res.map(Into::into))
    }
}

#[derive(Debug)]
pub struct DefaultErrorHandler {
    _priv: (),
}

impl DefaultErrorHandler {
    pub fn new() -> DefaultErrorHandler {
        DefaultErrorHandler { _priv: () }
    }
}

impl ErrorHandler for DefaultErrorHandler {
    fn handle_error(&self, err: Error, _: &Request<()>) -> Result<Response<ResponseBody>, CritError> {
        err.into_response()
    }
}
