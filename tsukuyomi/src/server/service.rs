#[doc(no_inline)]
pub use tower_service::{NewService, Service};

#[cfg(feature = "tower-middleware")]
pub use self::tower::Compat;

use {
    super::CritError,
    futures01::{Future, Poll},
    http::{Request, Response},
    hyper::body::{Body, Payload},
};

pub trait HttpService {
    type RequestBody: From<Body>;
    type ResponseBody: Payload;
    type Error: Into<CritError>;
    type Future: Future<Item = Response<Self::ResponseBody>, Error = Self::Error>;

    fn poll_ready_http(&mut self) -> Poll<(), Self::Error>;

    fn call_http(&mut self, request: Request<Self::RequestBody>) -> Self::Future;

    fn ready_http(self) -> ReadyHttp<Self>
    where
        Self: Sized,
    {
        ReadyHttp(Some(self))
    }
}

#[derive(Debug)]
pub struct ReadyHttp<S>(Option<S>);

impl<S> Future for ReadyHttp<S>
where
    S: HttpService,
{
    type Item = S;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        futures01::try_ready!(self
            .0
            .as_mut()
            .expect("the future has already polled")
            .poll_ready_http());
        Ok(futures01::Async::Ready(self.0.take().unwrap()))
    }
}

impl<S, RequestBody, ResponseBody> HttpService for S
where
    S: Service<Request = Request<RequestBody>, Response = Response<ResponseBody>>,
    RequestBody: From<Body>,
    ResponseBody: Payload,
    S::Error: Into<CritError>,
{
    type RequestBody = RequestBody;
    type ResponseBody = ResponseBody;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready_http(&mut self) -> Poll<(), Self::Error> {
        Service::poll_ready(self)
    }

    fn call_http(&mut self, request: Request<Self::RequestBody>) -> Self::Future {
        Service::call(self, request)
    }
}

pub trait MakeHttpService {
    type RequestBody: From<Body>;
    type ResponseBody: Payload;
    type Error: Into<CritError>;
    type Service: HttpService<
        RequestBody = Self::RequestBody,
        ResponseBody = Self::ResponseBody,
        Error = Self::Error,
    >;
    type InitError: Into<CritError>;
    type Future: Future<Item = Self::Service, Error = Self::InitError>;

    fn make_http_service(&self) -> Self::Future;
}

impl<S, RequestBody, ResponseBody> MakeHttpService for S
where
    S: NewService<Request = Request<RequestBody>, Response = Response<ResponseBody>>,
    RequestBody: From<Body>,
    ResponseBody: Payload,
    S::Error: Into<CritError>,
    S::InitError: Into<CritError>,
{
    type RequestBody = RequestBody;
    type ResponseBody = ResponseBody;
    type Error = S::Error;
    type Service = S::Service;
    type InitError = S::InitError;
    type Future = S::Future;

    fn make_http_service(&self) -> Self::Future {
        self.new_service()
    }
}
