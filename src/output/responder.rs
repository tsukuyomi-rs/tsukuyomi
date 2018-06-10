use http::header::HeaderValue;
use http::{header, Request, Response};

use error::Error;

use super::body::ResponseBody;
use super::output::Output;

/// A trait representing the conversion to an HTTP response.
pub trait Responder {
    /// Converts `self` to an HTTP response.
    fn respond_to<T>(self, request: &Request<T>) -> Result<Output, Error>;
}

impl Responder for Output {
    fn respond_to<T>(self, _: &Request<T>) -> Result<Output, Error> {
        Ok(self)
    }
}

impl<T> Responder for Response<T>
where
    T: Into<ResponseBody>,
{
    #[inline]
    fn respond_to<U>(self, _: &Request<U>) -> Result<Output, Error> {
        Ok(self.into())
    }
}

impl Responder for &'static str {
    #[inline]
    fn respond_to<T>(self, _: &Request<T>) -> Result<Output, Error> {
        Ok(text_response(self))
    }
}

impl Responder for String {
    #[inline]
    fn respond_to<T>(self, _: &Request<T>) -> Result<Output, Error> {
        Ok(text_response(self))
    }
}

fn text_response<T: Into<ResponseBody>>(body: T) -> Output {
    let mut response = Response::new(body.into());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.into()
}
