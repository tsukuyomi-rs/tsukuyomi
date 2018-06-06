use http::{header, Response};

use context::Context;
use error::Error;

use super::body::ResponseBody;
use super::output::Output;

pub trait Responder {
    fn respond_to(self, cx: &Context) -> Result<Output, Error>;
}

impl Responder for Output {
    fn respond_to(self, _cx: &Context) -> Result<Output, Error> {
        Ok(self)
    }
}

impl<T> Responder for Response<T>
where
    T: Into<ResponseBody>,
{
    fn respond_to(self, _cx: &Context) -> Result<Output, Error> {
        Ok(self.into())
    }
}

impl Responder for &'static str {
    fn respond_to(self, _cx: &Context) -> Result<Output, Error> {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(self)
            .map(Into::into)
            .map_err(Into::into)
    }
}

impl Responder for String {
    fn respond_to(self, _cx: &Context) -> Result<Output, Error> {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(self)
            .map(Into::into)
            .map_err(Into::into)
    }
}
