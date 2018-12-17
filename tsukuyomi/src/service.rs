//! A collection of traits that abstracts HTTP services.

#[doc(no_inline)]
pub use tower_service::Service;

use {
    futures01::{Future, Poll},
    http::{Request, Response},
};

/// A trait representing a factory of `Service`s.
///
/// The signature of this trait imitates `tower_util::MakeService` and will be replaced to it.
pub trait MakeService<Target, Request> {
    type Response;
    type Error;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn poll_ready(&mut self) -> Poll<(), Self::MakeError>;

    fn make_service(&self, target: Target) -> Self::Future;
}

pub trait HttpService<RequestBody> {
    type ResponseBody;
    type Error;
    type Future: Future<Item = Response<Self::ResponseBody>, Error = Self::Error>;

    fn poll_ready_http(&mut self) -> Poll<(), Self::Error>;

    fn call_http(&mut self, request: Request<RequestBody>) -> Self::Future;
}

impl<S, RequestBody, ResponseBody> HttpService<RequestBody> for S
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>>,
{
    type ResponseBody = ResponseBody;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready_http(&mut self) -> Poll<(), Self::Error> {
        Service::poll_ready(self)
    }

    fn call_http(&mut self, request: Request<RequestBody>) -> Self::Future {
        Service::call(self, request)
    }
}

pub trait MakeHttpService<Target, RequestBody> {
    type ResponseBody;
    type Error;
    type Service: HttpService<RequestBody, ResponseBody = Self::ResponseBody, Error = Self::Error>;
    type MakeError;
    type Future: Future<Item = Self::Service, Error = Self::MakeError>;

    fn poll_ready_http(&mut self) -> Poll<(), Self::MakeError>;

    fn make_http_service(&self, target: Target) -> Self::Future;
}

impl<S, Target, RequestBody, ResponseBody> MakeHttpService<Target, RequestBody> for S
where
    S: MakeService<Target, Request<RequestBody>, Response = Response<ResponseBody>>,
{
    type ResponseBody = ResponseBody;
    type Error = S::Error;
    type Service = S::Service;
    type MakeError = S::MakeError;
    type Future = S::Future;

    #[inline]
    fn poll_ready_http(&mut self) -> Poll<(), Self::MakeError> {
        MakeService::poll_ready(self)
    }

    #[inline]
    fn make_http_service(&self, target: Target) -> Self::Future {
        MakeService::make_service(self, target)
    }
}
