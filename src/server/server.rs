use failure::Error;
use futures::prelude::*;
use hyper::body::Body;
use hyper::server::conn::Http;
use hyper::service::{NewService, Service};
use std::mem;
use std::sync::Arc;
use tokio;
use tokio::runtime::{self, Runtime};

use super::conn::Connection;
use super::service::ServiceUpgradeExt;
use super::transport::{self, Io, Listener};

// TODO: impl Future
// TODO: configure for transports

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
        S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
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

#[derive(Debug)]
pub struct Server<S = ()> {
    listener: Listener,
    new_service: Arc<S>,
    protocol: Arc<Http>,
    runtime: Runtime,
}

impl Server<()> {
    pub fn builder() -> Builder {
        Builder::new()
    }
}

impl<S> Server<S>
where
    S: NewService<ReqBody = Body, ResBody = Body> + Send + Sync + 'static,
    S::Future: Send,
    S::Service: ServiceUpgradeExt<Io> + Send,
    <S::Service as Service>::Future: Send,
    <S::Service as ServiceUpgradeExt<Io>>::Upgrade: Send,
{
    /// Start a server using the supplied components.
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
                    let conn = Connection::Http(protocol.serve_connection(stream, service));
                    tokio::spawn(conn)
                })
            })
        });

        runtime.spawn(server);
        runtime.shutdown_on_idle().wait().unwrap();
    }
}
