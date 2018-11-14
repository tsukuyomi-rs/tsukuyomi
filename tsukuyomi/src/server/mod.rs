//! The implementation of low level HTTP server.

pub mod middleware;
pub mod transport;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll, Stream};
use http::{Request, Response};
use hyper;
use hyper::body::{Body, Payload};
use hyper::server::conn::Http;
use tower_service::{NewService, Service};

use self::middleware::{Chain, Middleware};
use self::transport::{ConnectionInfo, HasConnectionInfo, Transport};

/// A type alias representing a critical error.
pub type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

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

// ==== Server ====

#[allow(missing_debug_implementations)]
pub struct Server<S, Tr = SocketAddr> {
    new_service: S,
    transport: Tr,
    protocol: Http,
}

impl<S> Server<S>
where
    S: NewService,
{
    pub fn new(new_service: S) -> Self {
        Self {
            new_service,
            transport: ([127, 0, 0, 1], 4000).into(),
            protocol: Http::new(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, T> Server<S, T>
where
    S: NewService,
{
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000"` is set.
    pub fn bind<U>(self, transport: U) -> Server<S, U>
    where
        U: Transport,
    {
        Server {
            new_service: self.new_service,
            transport,
            protocol: self.protocol,
        }
    }

    /// Sets the HTTP-level configuration.
    pub fn protocol(self, protocol: Http) -> Self {
        Self { protocol, ..self }
    }

    pub fn with<M>(self, middleware: M) -> Server<Chain<S, M>, T>
    where
        M: Middleware<S::Service>,
    {
        Server {
            new_service: Chain::new(self.new_service, middleware),
            transport: self.transport,
            protocol: self.protocol,
        }
    }

    /// Starts a new `TestServer` using the contained value of `NewService`.
    ///
    /// Currently, the information about transport and protocol will be dropped.
    pub fn into_test_server(self) -> io::Result<crate::test::TestServer<S>>
    where
        S: Send + 'static,
        S::Request: HttpRequest,
        S::Response: HttpResponse,
        <S::Request as HttpRequest>::Body: From<Body>,
        <S::Response as HttpResponse>::Body: Payload,
        S::Error: Into<CritError>,
        S::Future: Send + 'static,
        S::Service: Send + 'static,
        <S::Service as Service>::Future: Send + 'static,
        S::InitError: Into<CritError> + Send + 'static,
    {
        crate::test::TestServer::new(self.new_service)
    }
}

macro_rules! serve {
    ($transport:expr, $new_service:expr, $protocol:expr, $spawn:expr) => {{
        let incoming = $transport.incoming()?;
        let new_service = $new_service;
        let protocol = $protocol;
        let spawn = $spawn;
        incoming
            .map_err(|_e| log::error!("incoming error"))
            .for_each(move |io| match io.fetch_info() {
                Ok(info) => {
                    let protocol = protocol.clone();
                    let future = new_service
                        .new_service()
                        .map_err(|_e| log::error!("new_service error"))
                        .and_then(move |service| {
                            let service = LiftedHttpService { service, info };
                            let conn = protocol
                                .serve_connection(io, service)
                                .with_upgrades()
                                .map_err(|_e| log::error!("connection error"));
                            spawn(conn);
                            Ok(())
                        });
                    futures::future::Either::A(future)
                }
                Err(err) => {
                    log::error!("failed to get connection info: {}", err);
                    futures::future::Either::B(futures::future::err(()))
                }
            })
    }};
    ($transport:expr, $new_service:expr, $protocol:expr, $spawn:expr, $signal:expr) => {
        serve!($transport, $new_service, $protocol, $spawn).select($signal.map_err(|_| ()))
    };
}

impl<S, T> Server<S, T>
where
    S: NewService,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    S::InitError: Into<CritError>,
    T: Transport,
    T::Data: Send + Sync + 'static,
{
    pub fn run_forever(self) -> io::Result<()>
    where
        S: Send + 'static,
        <S::Service as Service>::Future: Send + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
        T::Io: Send + 'static,
        T::Error: Into<CritError>,
        T::Incoming: Send + 'static,
    {
        let Self {
            new_service,
            transport,
            protocol,
        } = self;
        let protocol = std::sync::Arc::new(
            protocol.with_executor(tokio::executor::DefaultExecutor::current()),
        );
        let serve = serve!(transport, new_service, protocol, |fut| crate::rt::spawn(
            fut
        ));
        let runtime = tokio::runtime::Runtime::new()?;
        let _ = runtime.block_on_all(serve);
        Ok(())
    }

    pub fn run_until<F>(self, signal: F) -> io::Result<()>
    where
        S: Send + 'static,
        <S::Service as Service>::Future: Send + 'static,
        S::Service: Send + 'static,
        S::Future: Send + 'static,
        T::Io: Send + 'static,
        T::Error: Into<CritError>,
        T::Incoming: Send + 'static,
        F: Future<Item = ()> + Send + 'static,
    {
        let Self {
            new_service,
            transport,
            protocol,
        } = self;
        let protocol = std::sync::Arc::new(
            protocol.with_executor(tokio::executor::DefaultExecutor::current()),
        );
        let serve = serve!(
            transport,
            new_service,
            protocol,
            |fut| crate::rt::spawn(fut),
            signal
        );
        let runtime = tokio::runtime::Runtime::new()?;
        let _ = runtime.block_on_all(serve);
        Ok(())
    }

    pub fn run_single_threaded<F>(self, signal: F) -> io::Result<()>
    where
        S: 'static,
        <S::Service as Service>::Future: 'static,
        S::Service: 'static,
        S::Future: 'static,
        T::Io: Send + 'static,
        T::Error: Into<CritError>,
        T::Incoming: 'static,
        F: Future<Item = ()> + 'static,
    {
        use std::rc::Rc;
        use tokio::runtime::current_thread as rt;

        let Self {
            new_service,
            transport,
            protocol,
        } = self;
        let protocol = Rc::new(protocol.with_executor(rt::TaskExecutor::current()));
        let serve = serve!(
            transport,
            new_service,
            protocol,
            |fut| rt::spawn(fut),
            signal
        );
        let _ = rt::block_on_all(serve);
        Ok(())
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S, T> {
    service: S,
    info: T,
}

impl<S, T> hyper::service::Service for LiftedHttpService<S, T>
where
    S: Service,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    T: ConnectionInfo,
    T::Data: Send + Sync + 'static,
{
    type ReqBody = Body;
    type ResBody = <S::Response as HttpResponse>::Body;
    type Error = S::Error;
    type Future = LiftedHttpServiceFuture<S::Future>;

    #[inline]
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        request.extensions_mut().insert(self.info.data());
        let request =
            S::Request::from_request(request.map(<S::Request as HttpRequest>::Body::from));
        LiftedHttpServiceFuture(self.service.call(request))
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
    type Item = Response<<F::Item as HttpResponse>::Body>;
    type Error = F::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0
            .poll()
            .map(|x| x.map(|response| response.into_response()))
    }
}
