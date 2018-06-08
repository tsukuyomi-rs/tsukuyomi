pub(crate) mod service;

use failure::Error;
use std::mem;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::Path;
use std::sync::Arc;

use router::{self, Route, Router};
use rt;
#[cfg(feature = "tls")]
use transport::TlsConfig;
use transport::{self, Incoming, TransportConfig};

use self::service::NewAppService;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug)]
pub struct App {
    router: Arc<Router>,
    transport: transport::Builder,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
            transport: Incoming::builder(),
        }
    }

    pub fn serve(mut self) -> Result<()> {
        let incoming = self.transport.finish()?;
        rt::serve(NewAppService { app: self }, incoming)
    }
}

#[derive(Debug)]
pub struct AppBuilder {
    router: router::Builder,
    transport: transport::Builder,
}

impl AppBuilder {
    pub fn mount<I>(&mut self, base: &str, routes: I) -> &mut Self
    where
        I: IntoIterator<Item = Route>,
    {
        for route in routes {
            self.router.add_route(base, route);
        }
        self
    }

    pub fn bind_tcp(&mut self, addr: SocketAddr) -> &mut Self {
        self.transport.set_transport(TransportConfig::Tcp { addr: addr });
        self
    }

    #[cfg(unix)]
    pub fn bind_uds<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.transport.set_transport(TransportConfig::Uds {
            path: path.as_ref().to_owned(),
        });
        self
    }

    #[cfg(feature = "tls")]
    pub fn use_tls(&mut self, config: TlsConfig) -> &mut Self {
        self.transport.set_tls(config);
        self
    }

    pub fn finish(&mut self) -> Result<App> {
        Ok(App {
            router: self.router.finish().map(Arc::new)?,
            transport: mem::replace(&mut self.transport, Default::default()),
        })
    }

    pub fn serve(&mut self) -> Result<()> {
        self.finish()?.serve()
    }
}
