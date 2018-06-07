pub(crate) mod service;

use failure::Error;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use router::{self, Route, Router};
use {rt, transport};

use self::service::NewAppService;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug)]
pub enum TransportConfig {
    Tcp {
        addr: SocketAddr,
    },
    #[cfg(unix)]
    Uds {
        path: PathBuf,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig::Tcp {
            addr: ([127, 0, 0, 1], 4000).into(),
        }
    }
}

#[derive(Debug)]
pub struct App {
    router: Arc<Router>,
    transport: TransportConfig,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
            transport: None,
        }
    }

    pub fn serve(self) -> Result<()> {
        let incoming = match self.transport {
            TransportConfig::Tcp { ref addr } => transport::Incoming::tcp(addr)?,
            #[cfg(unix)]
            TransportConfig::Uds { ref path } => transport::Incoming::uds(path)?,
        };
        rt::serve(NewAppService { app: self }, incoming)
    }
}

#[derive(Debug)]
pub struct AppBuilder {
    router: router::Builder,
    transport: Option<TransportConfig>,
}

impl AppBuilder {
    pub fn mount<I>(&mut self, routes: I) -> &mut Self
    where
        I: IntoIterator<Item = Route>,
    {
        self.router.mount(routes);
        self
    }

    pub fn bind_tcp(&mut self, addr: SocketAddr) -> &mut Self {
        self.transport = Some(TransportConfig::Tcp { addr });
        self
    }

    #[cfg(unix)]
    pub fn bind_uds<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.transport = Some(TransportConfig::Uds {
            path: path.as_ref().to_owned(),
        });
        self
    }

    pub fn finish(&mut self) -> Result<App> {
        Ok(App {
            router: self.router.finish().map(Arc::new)?,
            transport: self.transport.take().unwrap_or_default(),
        })
    }

    pub fn serve(&mut self) -> Result<()> {
        self.finish()?.serve()
    }
}
