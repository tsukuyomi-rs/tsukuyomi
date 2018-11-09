#![allow(missing_docs)]

pub mod transport;
pub use self::transport::Transport;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll, Stream};
use http::{Request, Response};
use hyper;
use hyper::server::conn::Http;
use tower_service::{NewService, Service};

use self::transport::imp::{ConnectionInfo, HasConnectionInfo};
use service::http::imp::{HttpRequestImpl, HttpResponseImpl};
use service::http::{HttpRequest, HttpResponse, RequestBody};

/// A type alias representing a critical error.
pub type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Create a new `Server` from the specified `NewService`.
///
/// This function is a shortcut of `Server::new(new_service)`.
#[inline]
pub fn server<S>(new_service: S) -> Server<S, SocketAddr>
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
pub struct Server<S, Tr = SocketAddr> {
    new_service: S,
    transport: Tr,
    protocol: Http,
}

impl<S> Server<S> {
    pub fn new(new_service: S) -> Server<S> {
        Server {
            new_service,
            transport: ([127, 0, 0, 1], 4000).into(),
            protocol: Http::new(),
        }
    }
}

impl<S, T> Server<S, T> {
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000`" is set.
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

    /// Sets the HTTP-level configuration.
    pub fn protocol(self, protocol: Http) -> Server<S, T> {
        Server { protocol, ..self }
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
            .for_each(move |io| {
                let protocol = protocol.clone();
                new_service
                    .new_service()
                    .map_err(|_e| log::error!("new_service error"))
                    .and_then(move |service| {
                        let info = io.connection_info();
                        let service = LiftedHttpService { service, info };
                        let conn = protocol
                            .serve_connection(io, service)
                            .with_upgrades()
                            .map_err(|_e| log::error!("connection error"));
                        spawn(conn);
                        Ok(())
                    })
            })
    }};
    ($transport:expr, $new_service:expr, $protocol:expr, $spawn:expr, $signal:expr) => {
        serve!($transport, $new_service, $protocol, $spawn).select($signal.map_err(|_| ()))
    };
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
struct LiftedHttpService<S, Info> {
    service: S,
    info: Info,
}

impl<S, Info> hyper::service::Service for LiftedHttpService<S, Info>
where
    S: Service,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    Info: ConnectionInfo,
{
    type ReqBody = hyper::Body;
    type ResBody = <S::Response as HttpResponseImpl>::Body;
    type Error = S::Error;
    type Future = LiftedHttpServiceFuture<S::Future>;

    #[inline]
    fn call(&mut self, mut request: Request<hyper::Body>) -> Self::Future {
        self.info.insert_info(request.extensions_mut());
        let request = S::Request::from_request(request.map(RequestBody));
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
    type Item = Response<<F::Item as HttpResponseImpl>::Body>;
    type Error = F::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0
            .poll()
            .map(|x| x.map(|response| response.into_response()))
    }
}
