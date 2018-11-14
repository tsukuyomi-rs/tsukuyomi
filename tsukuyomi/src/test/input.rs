use http;
use http::header::HeaderValue;
use http::Request;
use hyper::body::Body;

// ==== traits ====

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait TestInput: TestInputImpl {}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait TestInputImpl {
    fn build_request(self) -> http::Result<Request<Body>>;
}

pub trait IntoRequestBody: IntoRequestBodyImpl {}

pub trait IntoRequestBodyImpl {
    fn content_type(&self) -> Option<HeaderValue> {
        None
    }

    fn into_request_body(self) -> Body;
}

// === implementors ===

impl<T: IntoRequestBody> TestInput for Request<T> {}
impl<T: IntoRequestBody> TestInputImpl for Request<T> {
    fn build_request(mut self) -> http::Result<Request<Body>> {
        if let Some(content_type) = self.body().content_type() {
            self.headers_mut().append("content-type", content_type);
        }
        Ok(self.map(IntoRequestBodyImpl::into_request_body))
    }
}

impl<T: IntoRequestBody, E: Into<http::Error>> TestInput for Result<Request<T>, E> {}
impl<T: IntoRequestBody, E: Into<http::Error>> TestInputImpl for Result<Request<T>, E> {
    fn build_request(self) -> http::Result<Request<Body>> {
        self.map_err(Into::into)?.build_request()
    }
}

impl TestInput for http::request::Builder {}
impl TestInputImpl for http::request::Builder {
    fn build_request(mut self) -> http::Result<Request<Body>> {
        (&mut self).build_request()
    }
}

impl<'a> TestInput for &'a mut http::request::Builder {}
impl<'a> TestInputImpl for &'a mut http::request::Builder {
    fn build_request(self) -> http::Result<Request<Body>> {
        self.body(Body::default())
    }
}

impl IntoRequestBody for () {}
impl IntoRequestBodyImpl for () {
    fn into_request_body(self) -> Body {
        Body::default()
    }
}

impl<'a> IntoRequestBody for &'a str {}
impl<'a> IntoRequestBodyImpl for &'a str {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("text/plain; charset=utf-8"))
    }
    fn into_request_body(self) -> Body {
        self.to_owned().into()
    }
}

impl IntoRequestBody for String {}
impl IntoRequestBodyImpl for String {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("text/plain; charset=utf-8"))
    }
    fn into_request_body(self) -> Body {
        self.into()
    }
}

impl<'a> IntoRequestBody for &'a [u8] {}
impl<'a> IntoRequestBodyImpl for &'a [u8] {
    fn into_request_body(self) -> Body {
        self.to_owned().into()
    }
}

impl IntoRequestBody for Vec<u8> {}
impl IntoRequestBodyImpl for Vec<u8> {
    fn into_request_body(self) -> Body {
        self.into()
    }
}
