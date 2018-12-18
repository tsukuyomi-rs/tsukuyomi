//! The implementation of low level HTTP server.

mod error;
mod io;
mod runtime;

pub(crate) type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use self::{
    error::{Error, Result},
    io::{Acceptor, Listener},
    runtime::Runtime,
};

#[doc(no_inline)]
pub use tower_service::Service;

use {
    futures01::{Future, Poll, Stream},
    http::{Request, Response},
    hyper::{
        body::{Body, Payload},
        server::conn::Http,
    },
    std::{marker::PhantomData, net::SocketAddr, rc::Rc, sync::Arc},
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

// ==== Server ====

/// An HTTP server.
#[derive(Debug)]
pub struct Server<S, L = SocketAddr, A = (), R = tokio::runtime::Runtime> {
    make_service: S,
    listener: L,
    acceptor: A,
    protocol: Http,
    runtime: Option<R>,
}

impl<S> Server<S>
where
    S: MakeService<(), hyper::Body>,
{
    /// Create a new `Server` with the specified `NewService` and default configuration.
    pub fn new(make_service: S) -> Self {
        Self {
            make_service,
            listener: ([127, 0, 0, 1], 4000).into(),
            acceptor: (),
            protocol: Http::new(),
            runtime: None,
        }
    }
}

impl<S, L, A, R> Server<S, L, A, R> {
    /// Sets the transport used by the server.
    ///
    /// By default, a TCP transport with the listener address `"127.0.0.1:4000"` is set.
    pub fn bind<L2>(self, listener: L2) -> Server<S, L2, A, R>
    where
        L2: Listener,
    {
        Server {
            make_service: self.make_service,
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
    pub fn acceptor<A2>(self, acceptor: A2) -> Server<S, L, A2, R>
    where
        L: Listener,
        A2: Acceptor<L::Conn>,
    {
        Server {
            make_service: self.make_service,
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

    /// Sets the instance of runtime to the specified `runtime`.
    pub fn runtime<R2>(self, runtime: R2) -> Server<S, L, A, R2> {
        Server {
            make_service: self.make_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: self.protocol,
            runtime: Some(runtime),
        }
    }

    /// Switches the runtime to be used to [`current_thread::Runtime`].
    ///
    /// [`current_thread::Runtime`]: https://docs.rs/tokio/0.1/tokio/runtime/current_thread/struct.Runtime.html
    pub fn current_thread(self) -> Server<S, L, A, tokio::runtime::current_thread::Runtime> {
        Server {
            make_service: self.make_service,
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
        make_service: $make_service:expr,
        listener: $listener:expr,
        acceptor: $acceptor:expr,
        protocol: $protocol:expr,
        spawn: $spawn:expr,
    ) => {{
        let make_service = $make_service;
        let listener = $listener;
        let acceptor = $acceptor;
        let protocol = $protocol;
        let spawn = $spawn;

        let incoming = listener
            .listen()
            .map_err(|err| failure::Error::from_boxed_compat(err.into()))?;

        ReadyMakeService(Some(make_service), PhantomData)
            .map_err(|e| log::error!("make_service init error: {}", e.into()))
            .and_then(move |make_service| {
                incoming
                    .map_err(|e| log::error!("transport error: {}", e.into()))
                    .for_each(move |io| {
                        let accept = acceptor
                            .accept(io)
                            .map_err(|e| log::error!("acceptor error: {}", e.into()));

                        let protocol = protocol.clone();
                        let service = make_service
                            .make_service(())
                            .map_err(|e| log::error!("make_service error: {}", e.into()));

                        let task = accept.and_then(move |io| {
                            service
                                .and_then(|service| {
                                    ReadyService(Some(service), PhantomData)
                                        .map_err(|e| log::error!("service error: {}", e.into()))
                                })
                                .and_then(move |service| {
                                    protocol
                                        .serve_connection(io, LiftedHttpService { service })
                                        .with_upgrades()
                                        .map_err(|e| log::error!("HTTP protocol error: {}", e))
                                })
                        });
                        spawn(task);
                        Ok(())
                    })
            })
    }};
}

impl<S, T, A, Bd> Server<S, T, A, tokio::runtime::Runtime>
where
    S: MakeService<(), Request<hyper::Body>, Response = Response<Bd>> + Send + 'static,
    Bd: Payload,
    S::Error: Into<CritError>,
    S::MakeError: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as Service<Request<hyper::Body>>>::Future: Send + 'static,
    T: Listener,
    T::Incoming: Send + 'static,
    A: Acceptor<T::Conn> + Send + 'static,
    A::Conn: Send + 'static,
    A::Error: Into<CritError>,
    A::Accept: Send + 'static,
{
    pub fn run(self) -> Result<()> {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::Runtime::new()?,
        };

        let serve = serve! {
            make_service: self.make_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: Arc::new(
                self.protocol.with_executor(tokio::executor::DefaultExecutor::current())
            ),
            spawn: |future| crate::rt::spawn(future),
        };

        Runtime::run(runtime, serve).map_err(Into::into)
    }
}

impl<S, T, A, Bd> Server<S, T, A, tokio::runtime::current_thread::Runtime>
where
    S: MakeService<(), Request<hyper::Body>, Response = Response<Bd>>,
    Bd: Payload,
    S::Error: Into<CritError>,
    S::MakeError: Into<CritError>,
    S::Future: 'static,
    S::Service: 'static,
    <S::Service as Service<Request<hyper::Body>>>::Future: 'static,
    T: Listener,
    T::Incoming: 'static,
    A: Acceptor<T::Conn> + 'static,
    A::Conn: Send + 'static,
    A::Error: Into<CritError>,
    A::Accept: 'static,
{
    pub fn run(self) -> Result<()> {
        let runtime = match self.runtime {
            Some(rt) => rt,
            None => tokio::runtime::current_thread::Runtime::new()?,
        };

        let serve = serve! {
            make_service: self.make_service,
            listener: self.listener,
            acceptor: self.acceptor,
            protocol: Rc::new(
                self.protocol.with_executor(tokio::runtime::current_thread::TaskExecutor::current())
            ),
            spawn: |future| tokio::runtime::current_thread::spawn(future),
        };

        Runtime::run(runtime, serve).map_err(Into::into)
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S> {
    service: S,
}

impl<S, Bd> hyper::service::Service for LiftedHttpService<S>
where
    S: Service<Request<hyper::Body>, Response = Response<Bd>>,
    Bd: Payload,
    S::Error: Into<CritError>,
{
    type ReqBody = Body;
    type ResBody = Bd;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        self.service.call(request)
    }
}

#[allow(missing_debug_implementations)]
struct ReadyMakeService<S, T, Req>(Option<S>, PhantomData<fn(T, Req)>);

impl<S, T, Req> Future for ReadyMakeService<S, T, Req>
where
    S: MakeService<T, Req>,
{
    type Item = S;
    type Error = S::MakeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        futures01::try_ready!(self
            .0
            .as_mut()
            .expect("the future has already been polled")
            .poll_ready());
        Ok(futures01::Async::Ready(self.0.take().unwrap()))
    }
}

#[allow(missing_debug_implementations)]
struct ReadyService<S, Req>(Option<S>, PhantomData<fn(Req)>);

impl<S, Req> Future for ReadyService<S, Req>
where
    S: Service<Req>,
{
    type Item = S;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        futures01::try_ready!(self
            .0
            .as_mut()
            .expect("the future has already been polled")
            .poll_ready());
        Ok(futures01::Async::Ready(self.0.take().unwrap()))
    }
}
