//! The definition of components for constructing the HTTP applications.

pub mod builder;
pub mod service;

mod endpoint;
mod recognizer;
mod scope;
mod uri;

#[cfg(test)]
mod tests;

use http::Method;
use indexmap::IndexMap;
use state::Container;
use std::fmt;
use std::sync::Arc;

use error::handler::ErrorHandler;
use error::Error;
use modifier::Modifier;

pub use self::builder::AppBuilder;
pub use self::endpoint::Endpoint;
use self::recognizer::Recognizer;
use self::scope::ScopedContainer;
pub use self::uri::Uri;

#[derive(Debug)]
struct Config {
    fallback_head: bool,
    _priv: (),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fallback_head: true,
            _priv: (),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Recognize {
    pub(crate) endpoint_id: usize,
    pub(crate) params: Vec<(usize, usize)>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ScopeId {
    Scope(usize),
    Global,
}

impl ScopeId {
    fn local_id(self) -> Option<usize> {
        match self {
            ScopeId::Scope(id) => Some(id),
            ScopeId::Global => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModifierId(ScopeId, usize);

struct ScopeData {
    parent: ScopeId,
    prefix: Option<Uri>,
    chain: Vec<ScopeId>,
    modifier_ids: Vec<ModifierId>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .field("chain", &self.chain)
            .finish()
    }
}

impl ScopeData {
    fn parent(&self) -> ScopeId {
        self.parent
    }
}

/// The global and shared variables used throughout the serving an HTTP application.
struct AppState {
    recognizer: Recognizer,
    routes: Vec<IndexMap<Method, usize>>,
    config: Config,
    endpoints: Vec<Endpoint>,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    container: Container,
    container_scoped: ScopedContainer,
    scopes: Vec<ScopeData>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppState")
            .field("endpoints", &self.endpoints)
            .field("scopes", &self.scopes)
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

    pub(crate) fn endpoint(&self, i: usize) -> Option<&Endpoint> {
        self.inner.endpoints.get(i)
    }

    pub(crate) fn error_handler(&self) -> &(dyn ErrorHandler + Send + Sync + 'static) {
        &*self.inner.error_handler
    }

    pub(crate) fn modifier(&self, id: ModifierId) -> Option<&(dyn Modifier + Send + Sync + 'static)> {
        let ModifierId(scope_id, pos) = id;
        match scope_id {
            ScopeId::Scope(id) => self.inner.scopes.get(id)?.modifiers.get(pos).map(|m| &**m),
            ScopeId::Global => self.inner.modifiers.get(pos).map(|m| &**m),
        }
    }

    pub(crate) fn get<T>(&self, id: ScopeId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        match id {
            ScopeId::Scope(id) => self.inner
                .container_scoped
                .get(id)
                .or_else(|| self.inner.container.try_get()),
            ScopeId::Global => self.inner.container.try_get(),
        }
    }

    fn recognize(&self, path: &str, method: &Method) -> Result<Recognize, Error> {
        let (i, params) = self.inner.recognizer.recognize(path).ok_or_else(Error::not_found)?;

        let methods = &self.inner.routes[i];
        match methods.get(method) {
            Some(&i) => Ok(Recognize { endpoint_id: i, params }),
            None if self.inner.config.fallback_head && *method == Method::HEAD => match methods.get(&Method::GET) {
                Some(&i) => Ok(Recognize { endpoint_id: i, params }),
                None => Err(Error::method_not_allowed()),
            },
            None => Err(Error::method_not_allowed()),
        }
    }
}
