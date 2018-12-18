//! The implementation of low level HTTP server.

pub mod io;
pub mod runtime;

mod error;

pub(crate) type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use self::error::{Error, Result};

use {
    self::{
        io::{Acceptor, Listener},
        runtime::Runtime,
    },
    crate::service::{HttpService, MakeHttpService},
    futures01::{Future, Poll, Stream},
    http::Request,
    hyper::{
        body::{Body, Payload},
        server::conn::Http,
    },
    std::{marker::PhantomData, net::SocketAddr, rc::Rc, sync::Arc},
};

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
    S: MakeHttpService<(), hyper::Body>,
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

        ReadyMakeHttp(Some(make_service), PhantomData)
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
                            .make_http_service(())
                            .map_err(|e| log::error!("make_service error: {}", e.into()));

                        let task = accept.and_then(move |io| {
                            service
                                .and_then(|service| {
                                    ReadyHttp(Some(service), PhantomData)
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

impl<S, T, A> Server<S, T, A, tokio::runtime::Runtime>
where
    S: MakeHttpService<(), hyper::Body> + Send + 'static,
    S::ResponseBody: Payload,
    S::Error: Into<CritError>,
    S::MakeError: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as HttpService<hyper::Body>>::Future: Send + 'static,
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

impl<S, T, A> Server<S, T, A, tokio::runtime::current_thread::Runtime>
where
    S: MakeHttpService<(), hyper::Body>,
    S::ResponseBody: Payload,
    S::Error: Into<CritError>,
    S::MakeError: Into<CritError>,
    S::Future: 'static,
    S::Service: 'static,
    <S::Service as HttpService<hyper::Body>>::Future: 'static,
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

impl<S> hyper::service::Service for LiftedHttpService<S>
where
    S: HttpService<hyper::Body>,
    S::ResponseBody: Payload,
    S::Error: Into<CritError>,
{
    type ReqBody = Body;
    type ResBody = S::ResponseBody;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        self.service.call_http(request)
    }
}

#[derive(Debug)]
pub struct ReadyMakeHttp<S, T, Bd>(Option<S>, PhantomData<fn(T, Bd)>);

impl<S, T, Bd> Future for ReadyMakeHttp<S, T, Bd>
where
    S: MakeHttpService<T, Bd>,
{
    type Item = S;
    type Error = S::MakeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        futures01::try_ready!(self
            .0
            .as_mut()
            .expect("the future has already been polled")
            .poll_ready_http());
        Ok(futures01::Async::Ready(self.0.take().unwrap()))
    }
}

#[derive(Debug)]
pub struct ReadyHttp<S, Bd>(Option<S>, PhantomData<fn(Bd)>);

impl<S, Bd> Future for ReadyHttp<S, Bd>
where
    S: HttpService<Bd>,
{
    type Item = S;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        futures01::try_ready!(self
            .0
            .as_mut()
            .expect("the future has already been polled")
            .poll_ready_http());
        Ok(futures01::Async::Ready(self.0.take().unwrap()))
    }
}
