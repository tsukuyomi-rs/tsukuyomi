use bytes::Bytes;
use failure::Error;
use futures::prelude::*;
use http::Request;
use hyper::body::{Body, Payload};
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use std::mem;
use std::sync::Arc;
use tokio;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::runtime::{self, Runtime};

use super::conn::Connection;
use super::service::ServiceUpgradeExt;
use super::transport::{self, Io, Listener};

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
    /// # use ganymede::server::Server;
    /// # use ganymede::App;
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
    /// # use ganymede::server::Server;
    /// # use ganymede::App;
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
    /// # extern crate ganymede;
    /// # use ganymede::server::Server;
    /// # use ganymede::App;
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
        S::Service: ServiceUpgradeExt<Io> + Send,
        <S::Service as Service>::Future: Send,
        <S::Service as ServiceUpgradeExt<Io>>::Upgrade: Send,
    {
        let mut builder = mem::replace(self, Builder::new());
        Ok(Server {
            listener: builder.transport.finish()?,
            new_service: Arc::new(new_service),
            protocol: Arc::new(builder.protocol),
            runtime: builder.runtime.build()?,
        })
    }
}

/// An HTTP server.
#[derive(Debug)]
pub struct Server<S = ()> {
    listener: Listener,
    new_service: Arc<S>,
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
    S::Service: ServiceUpgradeExt<Io> + Send,
    <S::Service as Service>::Future: Send,
    <S::Service as ServiceUpgradeExt<Io>>::Upgrade: Send,
{
    /// Starts a HTTP server using a configured runtime.
    pub fn serve(self) {
        let Server {
            new_service,
            listener,
            protocol,
            mut runtime,
        } = self;

        let server = listener.incoming().map_err(|_| ()).for_each(move |handshake| {
            let protocol = protocol.clone();
            let new_service = new_service.clone();
            handshake.map_err(|_| ()).and_then(move |stream| {
                new_service.new_service().map_err(|_e| ()).and_then(move |service| {
                    let conn = Connection::Http(protocol.serve_connection(stream, WrapService(service)));
                    tokio::spawn(conn)
                })
            })
        });

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

impl<S: Service, I: AsyncRead + AsyncWrite> ServiceUpgradeExt<I> for WrapService<S>
where
    S: Service + ServiceUpgradeExt<I>,
    S::ReqBody: From<Body>,
{
    type Upgrade = S::Upgrade;
    type UpgradeError = S::UpgradeError;

    fn poll_ready_upgradable(&mut self) -> Poll<(), Self::UpgradeError> {
        self.0.poll_ready_upgradable()
    }

    fn upgrade(self, io: I, read_buf: Bytes) -> Self::Upgrade {
        self.0.upgrade(io, read_buf)
    }
}
