//! Components for constructing HTTP applications.

#![cfg_attr(feature = "cargo-clippy", forbid(stutter))]

pub mod route;
pub mod scope;

mod builder;
mod callback;
mod error;
pub(crate) mod imp;
#[cfg(test)]
mod tests;

pub use {
    self::{
        builder::Builder,
        callback::Callback,
        error::{Error, Result},
        route::Route,
        scope::Scope,
    },
    crate::{route, scope},
};
use {
    self::{route::Handler, scope::Modifier},
    crate::{
        recognizer::Recognizer,
        scoped_map::{ScopeId, ScopedContainer},
        uri::Uri,
    },
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

pub fn route() -> self::route::Builder<()> {
    self::route::Builder::<()>::default()
}

pub fn scope() -> self::scope::Builder<()> {
    self::scope::Builder::<()>::default()
}

#[derive(Debug)]
struct Config {
    fallback_head: bool,
    fallback_options: bool,
    _priv: (),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fallback_head: true,
            fallback_options: true,
            _priv: (),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModifierId(ScopeId, usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteId(pub(crate) ScopeId, pub(crate) usize);

/// The global and shared variables used throughout the serving an HTTP application.
struct AppData {
    routes: Vec<RouteData>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,

    recognizer: Recognizer,
    endpoints: Vec<EndpointData>,

    states: ScopedContainer,
    callback: Box<dyn Callback + Send + Sync + 'static>,
    config: Config,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppData")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("global_scope", &self.global_scope)
            .field("recognizer", &self.recognizer)
            .field("endpoints", &self.endpoints)
            .field("states", &self.states)
            .field("config", &self.config)
            .finish()
    }
}

struct ScopeData {
    id: ScopeId,
    parent: ScopeId,
    prefix: Option<Uri>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .finish()
    }
}

impl ScopeData {
    fn modifier(&self, pos: usize) -> Option<&(dyn Modifier + Send + Sync + 'static)> {
        self.modifiers.get(pos).map(|m| &**m)
    }
}

struct RouteData {
    id: RouteId,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: Box<dyn Handler + Send + Sync + 'static>,
    modifier_ids: Vec<ModifierId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .field("modifier_ids", &self.modifier_ids)
            .finish()
    }
}

#[derive(Debug)]
struct EndpointData {
    route_ids: IndexMap<Method, usize>,
    allowed_methods: HeaderValue,
}

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    data: Arc<AppData>,
}

impl App {
    #[doc(hidden)]
    #[deprecated(note = "use `tsukuyomi::app` instead.")]
    pub fn builder() -> Builder {
        Builder::default()
    }

    pub(crate) fn uri(&self, id: RouteId) -> &Uri {
        &self.data.routes[id.1].uri
    }

    pub(crate) fn get_state<T>(&self, id: RouteId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.data.states.get(id.0)
    }

    fn get_scope(&self, id: ScopeId) -> Option<&ScopeData> {
        match id {
            ScopeId::Global => Some(&self.data.global_scope),
            ScopeId::Local(id) => self.data.scopes.get(id),
        }
    }

    fn get_route(&self, id: RouteId) -> Option<&RouteData> {
        self.data.routes.get(id.1)
    }

    fn find_modifier_by_pos(
        &self,
        route_id: RouteId,
        pos: usize,
    ) -> Option<&(dyn Modifier + Send + Sync + 'static)> {
        self.get_route(route_id)?
            .modifier_ids
            .get(pos)
            .and_then(|&id| self.get_scope(id.0)?.modifier(id.1))
    }
}
