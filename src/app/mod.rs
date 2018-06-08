pub mod service;

use failure::Error;
use std::sync::Arc;

use router::{self, Route, Router};

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

    pub fn finish(&mut self) -> Result<App, Error> {
        Ok(App {
            router: self.router.finish().map(Arc::new)?,
        })
    }
}
