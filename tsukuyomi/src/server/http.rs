use http::{Request, Response};

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait HttpRequest {
    type Body;

    fn from_request(request: Request<Self::Body>) -> Self;
}

impl<T> HttpRequest for Request<T> {
    type Body = T;

    #[inline]
    fn from_request(request: Self) -> Self {
        request
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait HttpResponse {
    type Body;

    fn into_response(self) -> Response<Self::Body>;
}

impl<T> HttpResponse for Response<T> {
    type Body = T;

    #[inline]
    fn into_response(self) -> Self {
        self
    }
}
