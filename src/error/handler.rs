use http::{header, Request, Response};

use output::ResponseBody;

use super::{CritError, HttpError};

pub trait ErrorHandler {
    fn handle_error(&self, err: &HttpError, request: &Request<()>) -> Result<Response<ResponseBody>, CritError>;
}

impl<F, T> ErrorHandler for F
where
    F: Fn(&HttpError, &Request<()>) -> Result<Response<T>, CritError>,
    T: Into<ResponseBody>,
{
    fn handle_error(&self, err: &HttpError, request: &Request<()>) -> Result<Response<ResponseBody>, CritError> {
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
    fn handle_error(&self, err: &HttpError, _: &Request<()>) -> Result<Response<ResponseBody>, CritError> {
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
