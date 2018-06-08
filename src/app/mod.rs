pub mod service;

use failure::Error;
use std::sync::Arc;

use router::{self, Route, Router};

#[derive(Debug)]
pub struct AppState {
    router: Router,
}

impl AppState {
    pub fn router(&self) -> &Router {
        &self.router
    }
}

#[derive(Debug)]
pub struct App {
    state: Arc<AppState>,
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
        let state = AppState {
            router: self.router.finish()?,
        };

        Ok(App { state: Arc::new(state) })
    }
}
