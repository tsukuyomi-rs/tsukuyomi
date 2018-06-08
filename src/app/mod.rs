pub(crate) mod service;

use failure::Error;
use std::sync::Arc;
use tokio;

use router::{self, Route, Router};
use server::Server;

use self::service::NewAppService;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug)]
pub struct App {
    router: Arc<Router>,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
        }
    }

    pub fn serve(self) -> Result<()> {
        let new_service = NewAppService { app: self };
        tokio::run(Server::new(new_service)?.serve());
        Ok(())
    }
}

#[derive(Debug)]
pub struct AppBuilder {
    router: router::Builder,
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

    pub fn finish(&mut self) -> Result<App> {
        Ok(App {
            router: self.router.finish().map(Arc::new)?,
        })
    }

    pub fn serve(&mut self) -> Result<()> {
        self.finish()?.serve()
    }
}
