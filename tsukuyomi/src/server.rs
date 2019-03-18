//! The implementation of HTTP server for tsukuyomi.

use {
    crate::app::App,
    futures01::Future,
    izanami::{h1::H1, net::tcp::AddrIncoming, service::ServiceExt},
    std::{io, net::ToSocketAddrs},
    tokio::runtime::Runtime,
};

#[allow(missing_debug_implementations)]
pub struct Server {
    app: App,
    runtime: Runtime,
}

impl Server {
    /// Creates a new `Server` using the specified `App`.
    pub fn new(app: App) -> io::Result<Self> {
        Ok(Self {
            app,
            runtime: Runtime::new()?,
        })
    }

    /// Spawns an HTTP server using the associated `App` onto the inner runtime.
    pub fn bind<A>(&mut self, addr: A) -> io::Result<()>
    where
        A: ToSocketAddrs,
    {
        let app = self.app.clone();
        self.runtime.spawn(
            izanami::server::Server::new(
                AddrIncoming::bind(addr)? //
                    .service_map(move |stream| {
                        H1::new() //
                            .serve(stream, app.new_service())
                    }),
            )
            .map_err(|e| eprintln!("server error: {}", e)),
        );

        Ok(())
    }

    /// Waits for the runtime until all spawned servers are completed.
    pub fn run_forever(self) {
        let mut entered = tokio_executor::enter()
            .expect("another executor has already set on the current thread");
        let shutdown = self.runtime.shutdown_on_idle();

        entered.block_on(shutdown).expect("never fail")
    }
}
