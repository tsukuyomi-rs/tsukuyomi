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

use error::handler::ErrorHandler;
use error::Error;
use handler::Handler;
use modifier::Modifier;
use recognizer::{Captures, Recognizer, Uri};

use self::builder::AppBuilder;
use self::scoped_map::ScopedMap;

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
    Global(usize),
    Scope(usize, usize),
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
struct AppState {
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
impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState")
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
    inner: Arc<AppState>,
}

impl App {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    fn route(&self, id: RouteId) -> Option<&RouteData> {
        let RouteId(_, pos) = id;
        self.inner.routes.get(pos)
    }

    fn error_handler(&self) -> &(dyn ErrorHandler + Send + Sync + 'static) {
        &*self.inner.error_handler
    }

    fn modifier(&self, id: ModifierId) -> Option<&(dyn Modifier + Send + Sync + 'static)> {
        match id {
            ModifierId::Global(pos) => self.inner.modifiers.get(pos).map(|m| &**m),
            ModifierId::Scope(id, pos) => self.inner.scopes.get(id)?.modifiers.get(pos).map(|m| &**m),
            ModifierId::Route(id, pos) => self.inner.routes.get(id)?.modifiers.get(pos).map(|m| &**m),
        }
    }

    pub(crate) fn get<T>(&self, id: RouteId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.inner.globals.get(id.0)
    }

    fn recognize(&self, path: &str, method: &Method) -> Result<(usize, Captures), Error> {
        let (i, params) = self.inner.recognizer.recognize(path).ok_or_else(Error::not_found)?;

        let methods = &self.inner.route_ids[i];
        match methods.get(method) {
            Some(&i) => Ok((i, params)),
            None if self.inner.config.fallback_head && *method == Method::HEAD => match methods.get(&Method::GET) {
                Some(&i) => Ok((i, params)),
                None => Err(Error::method_not_allowed()),
            },
            None => Err(Error::method_not_allowed()),
        }
    }
}
