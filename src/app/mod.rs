//! The definition of components for constructing the HTTP applications.

#![allow(missing_docs)]

pub mod builder;
pub mod service;

mod endpoint;
mod recognizer;
mod uri;

use fnv::FnvHashMap;
use http::header::HeaderValue;
use http::Method;
use state::Container;
use std::fmt;
use std::sync::Arc;

use error::handler::ErrorHandler;
use modifier::Modifier;

pub use self::builder::AppBuilder;
pub use self::endpoint::Endpoint;
use self::recognizer::Recognizer;
pub use self::uri::Uri;

#[derive(Debug)]
pub struct Config {
    pub fallback_head: bool,
    pub fallback_options: bool,
    _priv: (),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fallback_head: true,
            fallback_options: false,
            _priv: (),
        }
    }
}

/// The global and shared variables used throughout the serving an HTTP application.
struct AppState {
    recognizer: Recognizer,
    entries: Vec<RouterEntry>,
    endpoints: Vec<Endpoint>,
    config: Config,

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

    /// Gets the reference to i-th `Endpoint`.
    pub fn endpoint(&self, i: usize) -> Option<&Endpoint> {
        self.inner.endpoints.get(i)
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn error_handler(&self) -> &dyn ErrorHandler {
        &*self.inner.error_handler
    }

    /// Returns the reference to `ErrorHandler` contained in this value.
    pub fn modifiers(&self) -> &[Box<dyn Modifier + Send + Sync + 'static>] {
        &self.inner.modifiers
    }

    pub(crate) fn states(&self) -> &Container {
        &self.inner.states
    }
}

// ==== RouterEntry ====

#[derive(Debug)]
struct RouterEntry {
    routes: FnvHashMap<Method, usize>,
    allowed_methods: HeaderValue,
}

impl RouterEntry {
    fn get(&self, method: &Method) -> Option<usize> {
        self.routes.get(method).map(|&i| i)
    }

    fn allowed_methods(&self) -> HeaderValue {
        self.allowed_methods.clone()
    }
}
