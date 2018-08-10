//! The implementation of low level HTTP server.

pub mod local;
pub mod transport;

use failure::Error;
use futures::prelude::*;
use http::Request;
use hyper::body::{Body, Payload};
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use std::error;
use std::mem;
use std::sync::Arc;
use tokio;
use tokio::runtime::{self, Runtime};
pub use tokio_threadpool::{blocking, BlockingError};

use self::transport::Listener;

// ==== Server ====

/// A builder for constructing a `Server`.
#[derive(Debug)]
pub struct Builder {
    transport: transport::Builder,
    protocol: Http,
    runtime: runtime::Builder,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            transport: Listener::builder(),
            protocol: Http::new(),
            runtime: runtime::Builder::new(),
        }
    }

    /// Modifies the tranport level configurations.
    ///
    /// # Example
    ///
    /// ```
    /// # use tsukuyomi::server::Server;
    /// # use tsukuyomi::App;
    /// # let app = App::builder().finish().unwrap();
    /// let server = Server::builder()
    ///     .transport(|t| {
    ///         t.bind_tcp(([0, 0, 0, 0], 8888));
    ///     })
    ///     .finish(app).unwrap();
    /// ```
    pub fn transport(&mut self, f: impl FnOnce(&mut transport::Builder)) -> &mut Self {
        f(&mut self.transport);
        self
    }

    /// Modifies the HTTP level configurations.
    ///
    /// # Example
    ///
    /// ```
    /// # use tsukuyomi::server::Server;
    /// # use tsukuyomi::App;
    /// # let app = App::builder().finish().unwrap();
    /// let server = Server::builder()
    ///     .http(|http| {
    ///         http.http1_only(true)
    ///             .keep_alive(false);
    ///     })
    ///     .finish(app).unwrap();
    /// ```
    pub fn http(&mut self, f: impl FnOnce(&mut Http)) -> &mut Self {
        f(&mut self.protocol);
        self
    }

    /// Modifies the runtime level configurations.
    ///
    /// # Example
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate tsukuyomi;
    /// # use tsukuyomi::server::Server;
    /// # use tsukuyomi::App;
    /// # use tokio::executor::thread_pool::Builder as ThreadPoolBuilder;
    /// # let app = App::builder().finish().unwrap();
    /// let server = Server::builder()
    ///     .runtime(|rt| {
    ///         rt.threadpool_builder(ThreadPoolBuilder::new());
    ///     })
    ///     .finish(app).unwrap();
    /// ```
    pub fn runtime(&mut self, f: impl FnOnce(&mut runtime::Builder)) -> &mut Self {
        f(&mut self.runtime);
        self
    }

    /// Create an instance of configured `Server` with given `NewService`.
    pub fn finish<S>(&mut self, new_service: S) -> Result<Server<S>, Error>
    where
        S: NewService + Send + Sync + 'static,
        S::ReqBody: From<Body>,
        S::ResBody: Payload,
        S::Future: Send,
        S::Service: Send,
        <S::Service as Service>::Future: Send,
    {
        let mut builder = mem::replace(self, Builder::new());
        Ok(Server {
            listener: builder.transport.finish()?,
            new_service,
            protocol: Arc::new(builder.protocol),
            runtime: builder.runtime.build()?,
        })
    }
}

/// An HTTP server.
#[derive(Debug)]
pub struct Server<S = ()> {
    listener: Listener,
    new_service: S,
    protocol: Arc<Http>,
    runtime: Runtime,
}

impl Server<()> {
    /// Creates a builder object for constructing a value of this type.
    pub fn builder() -> Builder {
        Builder::new()
    }
}

impl<S> Server<S>
where
    S: NewService + Send + Sync + 'static,
    S::ReqBody: From<Body>,
    S::ResBody: Payload,
    S::Future: Send,
    S::InitError: Into<Box<dyn error::Error + Send + Sync + 'static>>,
    S::Service: Send,
    <S::Service as Service>::Future: Send,
{
    /// Starts a HTTP server using a configured runtime.
    pub fn serve(self) {
        let Server {
            new_service,
            listener,
            protocol,
            mut runtime,
        } = self;

        let server = listener
            .incoming()
            .map_err(|e| error!("during accepting the connection: {}", e))
            .for_each(move |handshake| {
                let handshake = handshake
                    .inspect(|_| trace!("handshake has done"))
                    .map_err(|e| error!("during processing the handshake: {}", e));

                let service = new_service
                    .new_service()
                    .inspect(|_| trace!("creating a new service"))
                    .map_err(|e| error!("at creating an instance of Service: {}", e.into()));

                let dispatch = handshake.join(service).and_then({
                    let protocol = protocol.clone();
                    move |(stream, service)| {
                        protocol
                            .serve_connection(stream, WrapService(service))
                            .with_upgrades()
                            .map_err(mem::drop)
                    }
                });

                trace!("spawn a task which manages a connection");
                tokio::spawn(dispatch)
            });

        trace!("spawn a server");
        runtime.spawn(server);
        runtime.shutdown_on_idle().wait().unwrap();
    }
}

struct WrapService<S>(S);

impl<S: Service> Service for WrapService<S>
where
    S: Service,
    S::ReqBody: From<Body>,
{
    type ReqBody = Body;
    type ResBody = S::ResBody;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        self.0.call(request.map(From::from))
    }
}
