//! The implementation of low level HTTP server.

pub mod middleware;

mod acceptor;
mod error;
mod http;
mod launcher;
mod transport;

pub(crate) type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use self::{
    acceptor::Acceptor,
    error::{Error, Result},
    http::{HttpRequest, HttpResponse},
    transport::{Peer, Transport},
};
use {
    self::{launcher::Launcher, middleware::Middleware},
    hyper::server::conn::Http,
    std::net::SocketAddr,
    tower_service::NewService,
};

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
            acceptor: self::acceptor::Raw::new(),
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
    A: Acceptor<T::Conn>,
{
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000"` is set.
    pub fn bind<T2>(self, transport: T2) -> Server<S, M, T2, A, L>
    where
        T2: Transport,
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

    pub fn acceptor<A2>(self, acceptor: A2) -> Server<S, M, T, A2, L>
    where
        A2: Acceptor<T::Conn>,
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

    pub fn with_middleware<M2>(self, middleware: M2) -> Server<S, M2, T, A, L>
    where
        M2: Middleware<S::Service>,
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
    pub fn with_tower_middleware<M2>(
        self,
        middleware: M2,
    ) -> Server<S, self::middleware::Compat<M2>, T, A, L>
    where
        M2: tower_web::middleware::Middleware<S::Service>,
    {
        self.with_middleware(self::middleware::Compat(middleware))
    }

    pub fn launcher<L2>(self, launcher: L2) -> Server<S, M, T, A, L2> {
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
