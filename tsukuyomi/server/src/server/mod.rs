#![allow(missing_docs)]

pub mod transport;
pub use self::transport::Transport;

use std::io;

use futures::{future, Future, Poll};
use http::{Request, Response};
use hyper;
use hyper::server::conn::Http;
use tower_service::{NewService, Service};

use rt;
use service::http::imp::{HttpRequestImpl, HttpResponseImpl};
use service::http::{HttpRequest, HttpResponse, RequestBody};

/// A type alias representing a critical error.
pub type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Create a new `Server` from the specified `NewService`.
///
/// This function is a shortcut of `Server::new(new_service)`.
#[inline]
pub fn server<S>(new_service: S) -> Server<S, ()>
where
    S: NewService,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::InitError: Into<CritError>,
{
    Server::new(new_service)
}

#[allow(missing_debug_implementations)]
pub struct Server<S, Tr = ()> {
    new_service: S,
    transport: Tr,
    protocol: Http,
}

impl<S> Server<S> {
    pub fn new(new_service: S) -> Server<S> {
        Server {
            new_service,
            transport: (),
            protocol: Http::new(),
        }
    }
}

impl<S, T> Server<S, T> {
    pub fn bind<Tr>(self, transport: Tr) -> Server<S, Tr>
    where
        Tr: Transport,
    {
        Server {
            new_service: self.new_service,
            transport,
            protocol: self.protocol,
        }
    }

    pub fn protocol(self, protocol: Http) -> Server<S, T> {
        Server { protocol, ..self }
    }
}

impl<S, Tr> Server<S, Tr>
where
    S: NewService,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::InitError: Into<CritError>,
    Tr: Transport,
{
    pub fn run_forever(self) -> io::Result<()>
    where
        S: Send + 'static,
        <S::Service as Service>::Future: Send + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
    {
        self.run_until(future::empty::<(), ()>())
    }

    pub fn run_until<F>(self, signal: F) -> io::Result<()>
    where
        S: Send + 'static,
        <S::Service as Service>::Future: Send + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
        F: Future<Item = ()> + Send + 'static,
    {
        let incoming = self.transport.incoming()?;
        let builder = hyper::server::Builder::new(incoming, self.protocol);
        let new_service = LiftedNewHttpService(self.new_service);
        let serve = builder
            .serve(new_service)
            .with_graceful_shutdown(signal)
            .map_err(|e| error!("{}", e));
        rt::run(serve);
        Ok(())
    }
}

#[allow(missing_debug_implementations)]
struct LiftedNewHttpService<S>(S);

impl<S> hyper::service::NewService for LiftedNewHttpService<S>
where
    S: NewService,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::InitError: Into<CritError>,
{
    type ReqBody = hyper::Body;
    type ResBody = <S::Response as HttpResponseImpl>::Body;
    type Error = S::Error;
    type Service = LiftedHttpService<S::Service>;
    type InitError = S::InitError;
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    type Future = future::Map<S::Future, fn(S::Service) -> LiftedHttpService<S::Service>>;

    fn new_service(&self) -> Self::Future {
        self.0.new_service().map(LiftedHttpService)
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S>(S);

impl<S> hyper::service::Service for LiftedHttpService<S>
where
    S: Service,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
{
    type ReqBody = hyper::Body;
    type ResBody = <S::Response as HttpResponseImpl>::Body;
    type Error = S::Error;
    type Future = LiftedHttpServiceFuture<S::Future>;

    #[inline]
    fn call(&mut self, request: Request<hyper::Body>) -> Self::Future {
        let request = S::Request::from_request(request.map(RequestBody));
        LiftedHttpServiceFuture(self.0.call(request))
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpServiceFuture<F>(F);

impl<F> Future for LiftedHttpServiceFuture<F>
where
    F: Future,
    F::Item: HttpResponse,
    F::Error: Into<CritError>,
{
    type Item = Response<<F::Item as HttpResponseImpl>::Body>;
    type Error = F::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0
            .poll()
            .map(|x| x.map(|response| response.into_response()))
    }
}
