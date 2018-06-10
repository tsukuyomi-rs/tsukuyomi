use http::header::{HeaderMap, HeaderName, HeaderValue};
use http::{Response, StatusCode};
use hyperx::header::Header;
use std::fmt;

/// [unstable]
/// A trait representing additional information for constructing an HTTP response.
pub trait HttpResponse {
    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }

    /// Appends some entries into the header map of an HTTP response.
    #[allow(unused_variables)]
    fn append_headers(&self, headers: &mut HeaderMap) {}
}

/// A set of extensions for `Response<T>`.
pub trait ResponseExt: sealed::Sealed {
    /// Inserts a typed header value to the header map of HTTP response.
    fn insert_header<H>(&mut self, val: H)
    where
        H: Header + fmt::Display;
}

impl<T> ResponseExt for Response<T> {
    fn insert_header<H>(&mut self, entry: H)
    where
        H: Header + fmt::Display,
    {
        // FIXME: use Header::fmt_header instead of Display::fmt.
        // FIXME: more efficiently

        let name = HeaderName::from_bytes(H::header_name().as_bytes()).unwrap();
        let value = HeaderValue::from_shared(entry.to_string().into()).unwrap();
        self.headers_mut().insert(name, value);
    }
}

mod sealed {
    use http::Response;

    pub trait Sealed {}

    impl<T> Sealed for Response<T> {}
}
