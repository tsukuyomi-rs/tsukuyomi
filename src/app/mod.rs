pub mod service;

use failure::Error;
use std::sync::Arc;
use std::{fmt, mem};

#[cfg(feature = "session")]
use cookie::Key;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use router::{self, Route, Router};

pub struct AppState {
    router: Router,
    error_handler: Box<ErrorHandler + Send + Sync + 'static>,
    #[cfg(feature = "session")]
    secret_key: Key,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").field("router", &self.router).finish()
    }
}

impl AppState {
    pub fn router(&self) -> &Router {
        &self.router
    }

    pub fn error_handler(&self) -> &ErrorHandler {
        &*self.error_handler
    }

    #[cfg(feature = "session")]
    pub fn secret_key(&self) -> &Key {
        &self.secret_key
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
            error_handler: None,
            #[cfg(feature = "session")]
            secret_key: None,
        }
    }

    pub fn state(&self) -> &AppState {
        &*self.state
    }
}

pub struct AppBuilder {
    router: router::Builder,
    error_handler: Option<Box<ErrorHandler + Send + Sync + 'static>>,
    #[cfg(feature = "session")]
    secret_key: Option<Key>,
}

impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder").field("router", &self.router).finish()
    }
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

    pub fn error_handler<H>(&mut self, error_handler: H) -> &mut Self
    where
        H: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    #[cfg(feature = "session")]
    pub fn secret_key<K>(&mut self, master_key: K) -> &mut Self
    where
        K: AsRef<[u8]>,
    {
        self.secret_key = Some(Key::from_master(master_key.as_ref()));
        self
    }

    pub fn finish(&mut self) -> Result<App, Error> {
        let mut builder = mem::replace(self, App::builder());

        let state = AppState {
            router: builder.router.finish()?,
            error_handler: builder
                .error_handler
                .unwrap_or_else(|| Box::new(DefaultErrorHandler::new())),
            #[cfg(feature = "session")]
            secret_key: builder.secret_key.unwrap_or_else(Key::generate),
        };

        Ok(App { state: Arc::new(state) })
    }
}
