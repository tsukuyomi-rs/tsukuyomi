use http::header::HeaderValue;
use http::{header, Response};

use error::Error;
use input::Input;

use super::body::ResponseBody;
use super::Output;

/// A trait representing the conversion to an HTTP response.
pub trait Responder {
    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input) -> Result<Output, Error>;
}

impl<T> Responder for Option<T>
where
    T: Responder,
{
    fn respond_to(self, input: &mut Input) -> Result<Output, Error> {
        self.ok_or_else(Error::not_found)?.respond_to(input)
    }
}

impl<T> Responder for Result<T, Error>
where
    T: Responder,
{
    fn respond_to(self, input: &mut Input) -> Result<Output, Error> {
        self?.respond_to(input)
    }
}

impl Responder for Output {
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(self)
    }
}

impl<T> Responder for Response<T>
where
    T: Into<ResponseBody>,
{
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(self.into())
    }
}

impl Responder for &'static str {
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(text_response(self))
    }
}

impl Responder for String {
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
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
