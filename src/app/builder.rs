//! Components for building an `App`.

use std::sync::Arc;
use std::{fmt, mem};

use failure::Error;
use state::Container;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use modifier::Modifier;

use super::router::{self, Mount};
use super::{App, AppState};

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    router: router::Builder,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: Container,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder").field("router", &self.router).finish()
    }
}

impl AppBuilder {
    pub(super) fn new() -> AppBuilder {
        AppBuilder {
            router: router::Builder::new(),
            error_handler: None,
            modifiers: vec![],
            states: Container::new(),
        }
    }

    /// Registers some handlers to the router, with mounting on the specified prefix.
    ///
    /// See the documentation of `Mount` for details.
    pub fn mount(&mut self, base: &str, f: impl FnOnce(&mut Mount)) -> &mut Self {
        self.router.mount(base, f);
        self
    }

    /// Modifies the router level configurations.
    pub fn router(&mut self, f: impl FnOnce(&mut router::Builder)) -> &mut Self {
        f(&mut self.router);
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<H>(&mut self, error_handler: H) -> &mut Self
    where
        H: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn modifier<M>(&mut self, modifier: M) -> &mut Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.modifiers.push(Box::new(modifier));
        self
    }

    /// Sets a value of `T` to the global storage.
    ///
    /// If a value of provided type has already set, this method drops `state` immediately
    /// and does not provide any affects to the global storage.
    pub fn manage<T>(&mut self, state: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.states.set(state);
        self
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(&mut self) -> Result<App, Error> {
        let mut builder = mem::replace(self, AppBuilder::new());
        builder.states.freeze();

        let global = AppState {
            router: builder.router.finish()?,
            error_handler: builder
                .error_handler
                .unwrap_or_else(|| Box::new(DefaultErrorHandler::new())),
            modifiers: builder.modifiers,
            states: builder.states,
        };

        Ok(App {
            global: Arc::new(global),
        })
    }
}
