//! The implementation of low level HTTP server.

pub mod acceptor;
pub mod connection_info;
mod error;
mod http;
pub mod launcher;
pub mod middleware;
pub mod transport;

use std::net::SocketAddr;

use hyper::server::conn::Http;
use tower_service::NewService;

use self::acceptor::Acceptor;
use self::connection_info::HasConnectionInfo;
use self::launcher::Launcher;
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
pub struct Server<
    S,
    M = self::middleware::Identity,
    T = SocketAddr,
    A = self::acceptor::Raw,
    L = self::launcher::DefaultLauncher,
> {
    new_service: S,
    middleware: M,
    transport: T,
    acceptor: A,
    protocol: Http,
    launcher: L,
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
            launcher: self::launcher::DefaultLauncher::default(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M, T, A, L> Server<S, M, T, A, L>
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
    pub fn bind<U>(self, transport: U) -> Server<S, M, U, A, L>
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
            launcher: self.launcher,
        }
    }

    pub fn acceptor<B>(self, acceptor: B) -> Server<S, M, T, B, L>
    where
        B: Acceptor<T::Io>,
    {
        Server {
            new_service: self.new_service,
            middleware: self.middleware,
            transport: self.transport,
            acceptor,
            protocol: self.protocol,
            launcher: self.launcher,
        }
    }

    /// Sets the HTTP-level configuration.
    pub fn protocol(self, protocol: Http) -> Self {
        Self { protocol, ..self }
    }

    pub fn with_middleware<N>(self, middleware: N) -> Server<S, N, T, A, L>
    where
        N: Middleware<S::Service>,
    {
        Server {
            new_service: self.new_service,
            middleware,
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: self.protocol,
            launcher: self.launcher,
        }
    }

    #[cfg(feature = "tower-middleware")]
    pub fn with_tower_middleware<N>(
        self,
        middleware: N,
    ) -> Server<S, self::middleware::Compat<N>, T, A, L>
    where
        N: tower_web::middleware::Middleware<S::Service>,
    {
        self.with_middleware(self::middleware::Compat(middleware))
    }

    pub fn launcher<Launcher>(self, launcher: Launcher) -> Server<S, M, T, A, Launcher> {
        Server {
            new_service: self.new_service,
            middleware: self.middleware,
            transport: self.transport,
            acceptor: self.acceptor,
            protocol: self.protocol,
            launcher,
        }
    }

    pub fn single_thread(self) -> Server<S, M, T, A, self::launcher::CurrentThread> {
        self.launcher(self::launcher::CurrentThread::default())
    }

    pub fn into_test_server(self) -> crate::test::Result<crate::test::TestServer<S, M>> {
        Ok(crate::test::TestServer::new(self.new_service)?.with_middleware(self.middleware))
    }
}

impl<S, M, T, A, L> Server<S, M, T, A, L> {
    pub fn run_forever(self) -> Result<()>
    where
        L: Launcher<S, M, T, A>,
    {
        self.launcher.launch(
            self.new_service,
            self.middleware,
            self.transport,
            self.acceptor,
            self.protocol,
        )
    }
}
