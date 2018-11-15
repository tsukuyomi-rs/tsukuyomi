//! The implementation of low level HTTP server.

pub mod acceptor;
pub mod connection_info;
mod error;
mod http;
pub mod middleware;
pub mod transport;

use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;

use futures::{Future, Poll, Stream};
use http::{Request, Response};
use hyper;
use hyper::body::{Body, Payload};
use hyper::server::conn::Http;
use tokio::executor::DefaultExecutor;
use tower_service::{NewService, Service};

use self::acceptor::Acceptor;
use self::connection_info::{ConnectionInfo, HasConnectionInfo};
use self::imp::CritError;
use self::middleware::Middleware;
use self::transport::Transport;

pub use self::error::{Error, Result};
pub use self::http::{HttpRequest, HttpResponse};

pub(crate) mod imp {
    use super::*;

    pub type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

    /// Create a new `Server` from the specified `NewService`.
    ///
    /// This function is a shortcut of `Server::new(new_service)`.
    #[inline]
    pub fn server<S>(new_service: S) -> Server<S>
    where
        S: NewService,
        S::Request: HttpRequest,
        S::Response: HttpResponse,
        S::Error: Into<CritError>,
        S::InitError: Into<CritError>,
    {
        Server::new(new_service)
    }
}

// ==== Server ====

#[allow(missing_debug_implementations)]
pub struct Server<S, M = self::middleware::Identity, Tr = SocketAddr, A = self::acceptor::Raw> {
    new_service: S,
    middleware: M,
    transport: Tr,
    acceptor: A,
    protocol: Http,
}

impl<S> Server<S>
where
    S: NewService,
{
    pub fn new(new_service: S) -> Self {
        Self {
            new_service,
            middleware: self::middleware::Identity::default(),
            transport: ([127, 0, 0, 1], 4000).into(),
            acceptor: self::acceptor::Raw::default(),
            protocol: Http::new(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M, T, A> Server<S, M, T, A>
where
    S: NewService,
    M: Middleware<S::Service>,
    T: Transport,
    T::Io: HasConnectionInfo,
    A: Acceptor<T::Io>,
{
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000"` is set.
    pub fn bind<U>(self, transport: U) -> Server<S, M, U, A>
    where
        U: Transport,
        U::Io: HasConnectionInfo,
    {
        Server {
            new_service: self.new_service,
            middleware: self.middleware,
            transport,
            acceptor: self.acceptor,
            protocol: self.protocol,
        }
    }

    pub fn acceptor<B>(self, acceptor: B) -> Server<S, M, T, B>
    where
        B: Acceptor<T::Io>,
    {
        Server {
            new_service: self.new_service,
            middleware: self.middleware,
            transport: self.transport,
            acceptor,
            protocol: self.protocol,
        }
    }

    /// Sets the HTTP-level configuration.
    pub fn protocol(self, protocol: Http) -> Self {
        Self { protocol, ..self }
    }

    pub fn with_middleware<N>(self, middleware: N) -> Server<S, N, T, A>
    where
        N: Middleware<S::Service>,
    {
        Server {
            new_service: self.new_service,
            middleware,
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: self.protocol,
        }
    }

    #[cfg(feature = "tower-middleware")]
    pub fn with_tower_middleware<N>(
        self,
        middleware: N,
    ) -> Server<S, self::middleware::Compat<N>, T, A>
    where
        N: tower_web::middleware::Middleware<S::Service>,
    {
        self.with_middleware(self::middleware::Compat(middleware))
    }

    pub fn into_test_server(self) -> crate::test::Result<crate::test::TestServer<S, M>> {
        Ok(crate::test::TestServer::new(self.new_service)?.with_middleware(self.middleware))
    }
}

macro_rules! serve {
    (
        new_service: $new_service:expr,
        middleware: $middleware:expr,
        transport: $transport:expr,
        acceptor: $acceptor:expr,
        protocol: $protocol:expr,
        spawn: $spawn:expr,
    ) => {{
        let new_service = $new_service;
        let middleware = $middleware;
        let transport = $transport;
        let acceptor = $acceptor;
        let protocol = $protocol;
        let spawn = $spawn;

        transport
            .incoming()
            .map_err(|err| failure::Error::from_boxed_compat(err.into()))?
            .map_err(|_e| log::error!("transport error"))
            .for_each(move |io| {
                let info = io.fetch_info();
                if let Err(..) = info {
                    log::error!("failed to fetch the connection information.");
                }
                let info = info.ok();

                let accept = acceptor
                    .accept(io)
                    .map_err(|_e| log::error!("acceptor error"));

                let middleware = middleware.clone();
                let protocol = protocol.clone();
                let service = new_service
                    .new_service()
                    .map_err(|_e| log::error!("new_service error"))
                    .map(move |service| middleware.wrap(service));

                let task = accept.and_then(move |io| {
                    service
                        .map(move |service| LiftedHttpService { service, info })
                        .and_then(move |service| {
                            protocol
                                .serve_connection(io, service)
                                .with_upgrades()
                                .map_err(|e| log::error!("HTTP protocol error: {}", e))
                        })
                });
                spawn(task);
                Ok(())
            })
    }};
}

impl<S, M, T, A> Server<S, M, T, A>
where
    S: NewService,
    S::InitError: Into<CritError>,
    M: Middleware<S::Service>,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    T: Transport,
    T::Io: HasConnectionInfo,
    <T::Io as HasConnectionInfo>::Data: Send + Sync + 'static,
    T::Error: Into<CritError>,
    A: Acceptor<T::Io>,
{
    pub fn run_forever(self) -> Result<()>
    where
        S: Send + 'static,
        S::Future: Send + 'static,
        M: Send + Sync + 'static,
        M::Service: Send + 'static,
        <M::Service as Service>::Future: Send + 'static,
        T::Io: Send + 'static,
        <T::Io as HasConnectionInfo>::Info: Send + 'static,
        T::Error: Into<CritError>,
        T::Incoming: Send + 'static,
        A: Send + 'static,
        A::Accepted: Send + 'static,
        A::Future: Send + 'static,
    {
        let serve = serve!{
            new_service: self.new_service,
            middleware: Arc::new(self.middleware),
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: Arc::new(
                self.protocol.with_executor(tokio::executor::DefaultExecutor::current()),
            ),
            spawn: |fut| crate::rt::spawn(fut),
        };

        let runtime = tokio::runtime::Runtime::new()?;
        let _ = runtime.block_on_all(serve);
        Ok(())
    }

    pub fn run_until<F>(self, signal: F) -> Result<()>
    where
        S: Send + 'static,
        S::Future: Send + 'static,
        M: Send + Sync + 'static,
        M::Service: Send + 'static,
        <M::Service as Service>::Future: Send + 'static,
        T::Io: Send + 'static,
        <T::Io as HasConnectionInfo>::Info: Send + 'static,
        T::Error: Into<CritError>,
        T::Incoming: Send + 'static,
        A: Send + 'static,
        A::Accepted: Send + 'static,
        A::Future: Send + 'static,
        F: Future<Item = ()> + Send + 'static,
    {
        let serve = serve!{
            new_service: self.new_service,
            middleware: std::sync::Arc::new(self.middleware),
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: std::sync::Arc::new(
                self.protocol.with_executor(DefaultExecutor::current()),
            ),
            spawn: |fut| crate::rt::spawn(fut),
        }.select(signal.map_err(|_| ()));

        let runtime = tokio::runtime::Runtime::new()?;
        let _ = runtime.block_on_all(serve);
        Ok(())
    }

    pub fn run_single_threaded<F>(self, signal: F) -> Result<()>
    where
        S: 'static,
        S::Future: 'static,
        M: 'static,
        M::Service: 'static,
        <M::Service as Service>::Future: 'static,
        T::Io: Send + 'static,
        <T::Io as HasConnectionInfo>::Info: 'static,
        T::Error: Into<CritError>,
        T::Incoming: 'static,
        A: 'static,
        A::Accepted: Send + 'static,
        A::Future: 'static,
        F: Future<Item = ()> + 'static,
    {
        use tokio::runtime::current_thread as rt;

        let serve = serve!{
            new_service: self.new_service,
            middleware: Rc::new(self.middleware),
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: Rc::new(self.protocol.with_executor(rt::TaskExecutor::current())),
            spawn: |fut| rt::spawn(fut),
        }.select(signal.map_err(|_| ()));

        let _ = rt::block_on_all(serve);
        Ok(())
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S, T> {
    service: S,
    info: Option<T>,
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
        if let Some(ref info) = self.info {
            request.extensions_mut().insert(info.data());
        }
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
