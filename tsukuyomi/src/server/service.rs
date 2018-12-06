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

/// A trait representing a *middleware*, which decorates a `Service`.
///
/// This trait has the same signature as `tower_web::middleware::Middleware`,
/// and eventually be replaced to it in the future version.
pub trait ModifyService<S> {
    type Request;
    type Response;
    type Error;
    type Service: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    fn modify_service(&self, service: S) -> Self::Service;

    fn chain<O>(self, outer: O) -> Chain<Self, O>
    where
        Self: Sized,
        O: ModifyService<Self::Service>,
    {
        Chain { inner: self, outer }
    }
}

#[derive(Debug, Default)]
pub struct Identity(());

impl<S: Service> ModifyService<S> for Identity {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;

    #[inline]
    fn modify_service(&self, service: S) -> Self::Service {
        service
    }
}

#[derive(Debug)]
pub struct Chain<I, O> {
    inner: I,
    outer: O,
}

impl<I, O, S> ModifyService<S> for Chain<I, O>
where
    S: Service,
    I: ModifyService<S>,
    O: ModifyService<I::Service>,
{
    type Request = O::Request;
    type Response = O::Response;
    type Error = O::Error;
    type Service = O::Service;

    #[inline]
    fn modify_service(&self, service: S) -> Self::Service {
        self.outer
            .modify_service(self.inner.modify_service(service))
    }
}

#[cfg(feature = "tower-middleware")]
mod tower {
    use tower_web::middleware as tower_middleware;

    #[derive(Debug, Clone)]
    pub struct Compat<M>(pub(crate) M);

    impl<M, S> super::ModifyService<S> for Compat<M>
    where
        M: tower_middleware::Middleware<S>,
    {
        type Request = M::Request;
        type Response = M::Response;
        type Error = M::Error;
        type Service = M::Service;

        #[inline]
        fn modify_service(&self, service: S) -> Self::Service {
            self.0.wrap(service)
        }
    }
}

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

pub trait ModifyHttpService<S> {
    type RequestBody: From<Body>;
    type ResponseBody: Payload;
    type Error: Into<CritError>;
    type Service: HttpService<
        RequestBody = Self::RequestBody,
        ResponseBody = Self::ResponseBody,
        Error = Self::Error,
    >;

    fn modify_http_service(&self, inner: S) -> Self::Service;
}

impl<M, S, RequestBody, ResponseBody> ModifyHttpService<S> for M
where
    M: ModifyService<S, Request = Request<RequestBody>, Response = Response<ResponseBody>>,
    S: HttpService,
    RequestBody: From<Body>,
    ResponseBody: Payload,
    M::Error: Into<CritError>,
{
    type RequestBody = RequestBody;
    type ResponseBody = ResponseBody;
    type Error = M::Error;
    type Service = M::Service;

    fn modify_http_service(&self, service: S) -> Self::Service {
        self.modify_service(service)
    }
}
