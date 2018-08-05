//! Components for constructing HTTP applications.

pub mod builder;
pub mod service;

#[macro_use]
mod scoped_map;

#[cfg(test)]
mod tests;

use http::Method;
use indexmap::IndexMap;
use std::fmt;
use std::sync::Arc;

use error::ErrorHandler;
use handler::Handler;
use modifier::Modifier;
use recognizer::{Recognizer, Uri};

use self::builder::AppBuilder;
use self::scoped_map::ScopedMap;
pub use self::service::RecognizeError;

#[derive(Debug)]
struct Config {
    fallback_head: bool,
    fallback_options: bool,
    _priv: (),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fallback_head: true,
            fallback_options: true,
            _priv: (),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ScopeId {
    Global,
    Local(usize),
}

impl ScopeId {
    fn local_id(self) -> Option<usize> {
        match self {
            ScopeId::Global => None,
            ScopeId::Local(id) => Some(id),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ModifierId {
    Scope(ScopeId, usize),
    Route(usize, usize),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteId(ScopeId, usize);

struct ScopeData {
    id: ScopeId,
    parent: ScopeId,
    prefix: Option<Uri>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .finish()
    }
}

struct RouteData {
    id: RouteId,
    uri: Uri,
    method: Method,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    handler: Box<dyn Handler + Send + Sync + 'static>,
    modifier_ids: Vec<ModifierId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("method", &self.method)
            .field("modifier_ids", &self.modifier_ids)
            .finish()
    }
}

/// The global and shared variables used throughout the serving an HTTP application.
struct AppData {
    routes: Vec<RouteData>,
    scopes: Vec<ScopeData>,

    recognizer: Recognizer,
    route_ids: Vec<IndexMap<Method, usize>>,
    config: Config,

    globals: ScopedMap,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppData")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("recognizer", &self.recognizer)
            .field("route_ids", &self.route_ids)
            .field("config", &self.config)
            .field("globals", &self.globals)
            .finish()
    }
}

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    data: Arc<AppData>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    pub(crate) fn get_state<T>(&self, id: RouteId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.data.globals.get(id.0)
    }
}
