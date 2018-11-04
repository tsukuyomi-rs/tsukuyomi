use http;
use http::header::HeaderValue;
use http::Request;

use service::http::RequestBody;

// ==== traits ====

pub trait Input: InputImpl {}

pub trait InputImpl {
    fn build_request(self) -> http::Result<Request<RequestBody>>;
}

pub trait IntoRequestBody: IntoRequestBodyImpl {}

pub trait IntoRequestBodyImpl {
    fn content_type(&self) -> Option<HeaderValue> {
        None
    }
    fn into_request_body(self) -> RequestBody;
}

// === implementors ===

impl<T: IntoRequestBody> Input for Request<T> {}
impl<T: IntoRequestBody> InputImpl for Request<T> {
    fn build_request(mut self) -> http::Result<Request<RequestBody>> {
        if let Some(content_type) = self.body().content_type() {
            self.headers_mut().append("content-type", content_type);
        }
        Ok(self.map(IntoRequestBodyImpl::into_request_body))
    }
}

impl<T: IntoRequestBody, E: Into<http::Error>> Input for Result<Request<T>, E> {}
impl<T: IntoRequestBody, E: Into<http::Error>> InputImpl for Result<Request<T>, E> {
    fn build_request(self) -> http::Result<Request<RequestBody>> {
        self.map_err(Into::into)?.build_request()
    }
}

impl Input for http::request::Builder {}
impl InputImpl for http::request::Builder {
    fn build_request(mut self) -> http::Result<Request<RequestBody>> {
        (&mut self).build_request()
    }
}

impl<'a> Input for &'a mut http::request::Builder {}
impl<'a> InputImpl for &'a mut http::request::Builder {
    fn build_request(self) -> http::Result<Request<RequestBody>> {
        self.body(RequestBody(Default::default()))
    }
}

impl IntoRequestBody for () {}
impl IntoRequestBodyImpl for () {
    fn into_request_body(self) -> RequestBody {
        RequestBody(Default::default())
    }
}

impl<'a> IntoRequestBody for &'a str {}
impl<'a> IntoRequestBodyImpl for &'a str {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("text/plain; charset=utf-8"))
    }
    fn into_request_body(self) -> RequestBody {
        RequestBody(self.to_owned().into())
    }
}

impl IntoRequestBody for String {}
impl IntoRequestBodyImpl for String {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("text/plain; charset=utf-8"))
    }
    fn into_request_body(self) -> RequestBody {
        RequestBody(self.into())
    }
}

impl<'a> IntoRequestBody for &'a [u8] {}
impl<'a> IntoRequestBodyImpl for &'a [u8] {
    fn into_request_body(self) -> RequestBody {
        RequestBody(self.to_owned().into())
    }
}

impl IntoRequestBody for Vec<u8> {}
impl IntoRequestBodyImpl for Vec<u8> {
    fn into_request_body(self) -> RequestBody {
        RequestBody(self.into())
    }
}
