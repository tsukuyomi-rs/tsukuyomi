//! The definition of components for constructing the HTTP applications.

pub mod builder;
pub mod router;
pub mod service;

mod endpoint;
mod recognizer;
mod uri;

pub use self::endpoint::Endpoint;
pub use self::uri::Uri;

use state::Container;
use std::fmt;
use std::sync::Arc;

use error::handler::ErrorHandler;
use modifier::Modifier;

pub use self::builder::AppBuilder;
use self::router::Router;

/// The global and shared variables used throughout the serving an HTTP application.
pub struct AppState {
    router: Router,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: Container,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState").field("router", &self.router).finish()
    }
}

impl AppState {
    /// Returns the reference to `Router` contained in this value.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn error_handler(&self) -> &dyn ErrorHandler {
        &*self.error_handler
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn modifiers(&self) -> &[Box<dyn Modifier + Send + Sync + 'static>] {
        &self.modifiers
    }

    /// Returns the reference to a value of `T` from the global storage.
    ///
    /// # Panics
    /// If the value is not registered, it will cause a panic.
    pub fn get<T>(&self) -> &T
    where
        T: Send + Sync + 'static,
    {
        self.states.get()
    }

    /// Returns the reference to a value of `T` from the global storage.
    ///
    /// If the value is not registered, it returns a `None`.
    pub fn try_get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.states.try_get()
    }
}

/// The main type which represents an HTTP application.
#[derive(Debug)]
pub struct App {
    global: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }
}
