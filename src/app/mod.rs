//! The definition of components for constructing the HTTP applications.

#![allow(missing_docs)]

pub mod builder;
pub mod service;

mod endpoint;
mod router;
mod scope;
mod uri;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use error::handler::ErrorHandler;
use modifier::Modifier;

pub use self::builder::AppBuilder;
pub use self::endpoint::Endpoint;
use self::router::Router;
use self::scope::Container;
pub use self::uri::Uri;

/// The global and shared variables used throughout the serving an HTTP application.
struct AppState {
    router: Router,
    endpoints: Vec<Endpoint>,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: Container,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").finish()
    }
}

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    inner: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    pub(crate) fn endpoint(&self, i: usize) -> Option<&Endpoint> {
        self.inner.endpoints.get(i)
    }

    pub(crate) fn error_handler(&self) -> &dyn ErrorHandler {
        &*self.inner.error_handler
    }

    pub(crate) fn modifiers(&self) -> &[Box<dyn Modifier + Send + Sync + 'static>] {
        &self.inner.modifiers
    }

    pub(crate) fn states(&self) -> &Container {
        &self.inner.states
    }

    fn router(&self) -> &Router {
        &self.inner.router
    }
}
