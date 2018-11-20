//! The implementation of low level HTTP server.

pub mod io;
pub mod runtime;
pub mod service;

mod error;

pub(crate) type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use self::error::{Error, Result};

use {
    self::{
        io::{Acceptor, Connection, ConnectionInfo, Listener},
        runtime::Runtime,
        service::{HttpService, MakeHttpService, ModifyHttpService, ModifyService, NewService},
    },
    futures::{Future, Stream},
    http::Request,
    hyper::{body::Body, server::conn::Http},
    std::{net::SocketAddr, rc::Rc, sync::Arc},
};

// ==== Server ====

/// An HTTP server.
#[derive(Debug)]
pub struct Server<
    S,
    M = self::service::Identity,
    L = SocketAddr,
    A = (),
    R = tokio::runtime::Runtime,
> {
    new_service: S,
    modify_service: M,
    listener: L,
    acceptor: A,
    protocol: Http,
    runtime: Option<R>,
}

impl<S> Server<S>
where
    S: NewService,
{
    /// Create a new `Server` with the specified `NewService` and default configuration.
    pub fn new(new_service: S) -> Self {
        Self {
            new_service,
            modify_service: self::service::Identity::default(),
            listener: ([127, 0, 0, 1], 4000).into(),
            acceptor: (),
            protocol: Http::new(),
            runtime: None,
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M, L, A, R> Server<S, M, L, A, R> {
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000"` is set.
    pub fn bind<L2>(self, listener: L2) -> Server<S, M, L2, A, R>
    where
        L2: Listener,
    {
        Server {
            new_service: self.new_service,
            modify_service: self.modify_service,
            listener,
            acceptor: self.acceptor,
            protocol: self.protocol,
            runtime: self.runtime,
        }
    }

    /// Sets the instance of `Acceptor` to the server.
    ///
    /// By default, the raw acceptor is set, which returns the incoming
    /// I/Os directly.
    pub fn acceptor<A2>(self, acceptor: A2) -> Server<S, M, L, A2, R>
    where
        L: Listener,
        A2: Acceptor<L::Conn>,
    {
        Server {
            new_service: self.new_service,
            modify_service: self.modify_service,
            listener: self.listener,
            acceptor,
            protocol: self.protocol,
            runtime: self.runtime,
        }
    }

    /// Sets the HTTP-level configuration to this server.
    ///
    /// Note that the executor will be overwritten by the launcher.
    pub fn protocol(self, protocol: Http) -> Self {
        Self { protocol, ..self }
    }

    /// Sets the middleware to this server.
    pub fn modify_service<M2>(self, modify_service: M2) -> Server<S, M2, L, A, R>
    where
        S: NewService,
        M2: ModifyService<S::Service>,
    {
        Server {
            new_service: self.new_service,
            modify_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: self.protocol,
            runtime: self.runtime,
        }
    }

    #[cfg(feature = "tower-middleware")]
    pub fn tower_middleware<M2>(
        self,
        middleware: M2,
    ) -> Server<S, self::service::Compat<M2>, L, A, R>
    where
        S: NewService,
        M2: tower_web::middleware::Middleware<S::Service>,
    {
        self.modify_service(self::service::Compat(middleware))
    }

    /// Sets the instance of runtime to the specified `runtime`.
    pub fn runtime<R2>(self, runtime: R2) -> Server<S, M, L, A, R2> {
        Server {
            new_service: self.new_service,
            modify_service: self.modify_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: self.protocol,
            runtime: Some(runtime),
        }
    }

    /// Switches the runtime to be used to [`current_thread::Runtime`].
    ///
    /// [`current_thread::Runtime`]: https://docs.rs/tokio/0.1/tokio/runtime/current_thread/struct.Runtime.html
    pub fn current_thread(self) -> Server<S, M, L, A, tokio::runtime::current_thread::Runtime> {
        Server {
            new_service: self.new_service,
            modify_service: self.modify_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: self.protocol,
            runtime: None,
        }
    }
}

/// A macro for creating a server task from the specified components.
macro_rules! serve {
    (
        new_service: $new_service:expr,
        modify_service: $modify_service:expr,
        listener: $listener:expr,
        acceptor: $acceptor:expr,
        protocol: $protocol:expr,
        spawn: $spawn:expr,
    ) => {{
        let new_service = $new_service;
        let modify_service = $modify_service;
        let listener = $listener;
        let acceptor = $acceptor;
        let protocol = $protocol;
        let spawn = $spawn;

        let incoming = listener
            .listen()
            .map_err(|err| failure::Error::from_boxed_compat(err.into()))?;

        incoming
            .map_err(|e| log::error!("transport error: {}", e.into()))
            .for_each(move |io| {
                let accept = acceptor
                    .accept(io)
                    .map_err(|e| log::error!("acceptor error: {}", e.into()));

                let modify_service = modify_service.clone();
                let protocol = protocol.clone();
                let service = new_service
                    .make_http_service()
                    .map_err(|e| log::error!("new_service error: {}", e.into()))
                    .map(move |service| modify_service.modify_http_service(service));

                let task = accept.and_then(move |io| {
                    let info = io.connection_info();
                    if let Err(..) = info {
                        log::error!("failed to fetch the connection information");
                    }
                    let info = info.ok();

                    service
                        .and_then(|service| {
                            service
                                .ready_http()
                                .map_err(|e| log::error!("service error: {}", e.into()))
                        }).map(move |service| LiftedHttpService { service, info })
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

impl<S, M, T, A> Server<S, M, T, A, tokio::runtime::Runtime>
where
    S: MakeHttpService + Send + 'static,
    S::Future: Send + 'static,
    M: ModifyHttpService<S::Service> + Send + Sync + 'static,
    M::Service: Send + 'static,
    <M::Service as HttpService>::Future: Send + 'static,
    T: Listener,
    T::Incoming: Send + 'static,
    A: Acceptor<T::Conn> + Send + 'static,
    A::Conn: Send + 'static,
    A::Error: Into<CritError>,
    <A::Conn as Connection>::Info: Send + 'static,
    <A::Conn as Connection>::Error: Into<CritError>,
    A::Accept: Send + 'static,
{
    pub fn run(self) -> Result<()> {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::Runtime::new()?,
        };

        let serve = serve! {
            new_service: self.new_service,
            modify_service: Arc::new(self.modify_service),
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: Arc::new(
                self.protocol.with_executor(tokio::executor::DefaultExecutor::current())
            ),
            spawn: |future| crate::rt::spawn(future),
        };

        Runtime::run(runtime, serve).map_err(Into::into)
    }

    /// Convert itself into a `TestServer`.
    pub fn into_test_server(self) -> crate::test::Result<crate::test::Server<S, M>> {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::Builder::new()
                .core_threads(1)
                .blocking_threads(1)
                .build()?,
        };
        Ok(crate::test::Server::new(
            self.new_service,
            self.modify_service,
            runtime,
        ))
    }
}

impl<S, M, T, A> Server<S, M, T, A, tokio::runtime::current_thread::Runtime>
where
    S: MakeHttpService,
    S::Future: 'static,
    M: ModifyHttpService<S::Service> + 'static,
    M::Service: 'static,
    <M::Service as HttpService>::Future: 'static,
    T: Listener,
    T::Incoming: 'static,
    A: Acceptor<T::Conn> + 'static,
    A::Conn: Send + 'static,
    A::Error: Into<CritError>,
    <A::Conn as Connection>::Info: 'static,
    <A::Conn as Connection>::Error: Into<CritError>,
    A::Accept: 'static,
{
    pub fn run(self) -> Result<()> {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::current_thread::Runtime::new()?,
        };

        let serve = serve! {
            new_service: self.new_service,
            modify_service: Rc::new(self.modify_service),
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: Rc::new(
                self.protocol.with_executor(tokio::runtime::current_thread::TaskExecutor::current())
            ),
            spawn: |future| tokio::runtime::current_thread::spawn(future),
        };

        Runtime::run(runtime, serve).map_err(Into::into)
    }

    /// Convert itself into a `TestServer`.
    pub fn into_test_server(
        self,
    ) -> crate::test::Result<crate::test::Server<S, M, tokio::runtime::current_thread::Runtime>>
    {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::current_thread::Runtime::new()?,
        };
        Ok(crate::test::Server::new(
            self.new_service,
            self.modify_service,
            runtime,
        ))
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S, T> {
    service: S,
    info: Option<T>,
}

impl<S, T> hyper::service::Service for LiftedHttpService<S, T>
where
    S: HttpService,
    T: ConnectionInfo,
{
    type ReqBody = Body;
    type ResBody = S::ResponseBody;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if let Some(ref info) = self.info {
            info.insert_into(request.extensions_mut());
        }
        self.service.call_http(request.map(S::RequestBody::from))
    }
}
