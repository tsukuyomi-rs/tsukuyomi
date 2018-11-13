//! Components for constructing HTTP applications.

#![cfg_attr(feature = "cargo-clippy", forbid(stutter))]

mod builder;
mod error;
pub mod global;
mod handler;
pub(crate) mod imp;
pub mod route;
pub mod scope;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use http::Method;
use indexmap::{IndexMap, IndexSet};

use crate::error::handler::ErrorHandler;
use crate::internal::recognizer::Recognizer;
use crate::internal::scoped_map::{ScopeId, ScopedContainer};
use crate::internal::uri::Uri;

pub use self::error::{Error, Result};
pub use self::global::Global;
pub use self::handler::{AsyncResult, Handler, Modifier};
pub use self::imp::RecognizeError;
pub use self::route::Route;
pub use self::scope::Scope;

pub use crate::route;

pub fn route() -> self::route::Builder<()> {
    self::route::Builder::<()>::default()
}

pub fn scope() -> self::scope::Builder<()> {
    self::scope::Builder::<()>::default()
}

pub fn global() -> self::global::Builder<()> {
    self::global::Builder::<()>::default()
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
    route_ids: Vec<IndexMap<Method, usize>>,

    states: ScopedContainer,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
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
            .field("route_ids", &self.route_ids)
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

    fn uri(&self, id: RouteId) -> &Uri {
        &self.data.routes[id.1].uri
    }

    fn get_state<T>(&self, id: RouteId) -> Option<&T>
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

/// A builder object for constructing an instance of `App`.
#[derive(Debug, Default)]
pub struct Builder<S: Scope = (), G: Global = ()> {
    scope: S,
    global: G,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, G> Builder<S, G>
where
    S: Scope,
    G: Global,
{
    /// Adds a route into the global scope.
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>, G> {
        let Self { scope, global } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.add_route(route)?;
                Ok(())
            }),
        }
    }

    /// Creates a new scope onto the global scope using the specified `Scope`.
    pub fn mount(self, new_scope: impl Scope) -> Builder<impl Scope<Error = Error>, G> {
        let Self { global, scope } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.add_scope(new_scope)?;
                Ok(())
            }),
        }
    }

    /// Merges the specified `Scope` into the global scope, *without* creating a new scope.
    pub fn with(self, next: impl Scope) -> Builder<impl Scope<Error = Error>, G> {
        let Self {
            global,
            scope: current,
        } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                current.configure(cx).map_err(Into::into)?;
                next.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }

    /// Adds a *global* variable into the application.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>, G>
    where
        T: Send + Sync + 'static,
    {
        let Self { scope, global } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                scope.configure(cx)?;
                cx.set_state(state);
                Ok(())
            }),
        }
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier<M>(self, modifier: M) -> Builder<impl Scope<Error = S::Error>, G>
    where
        M: Modifier + Send + Sync + 'static,
    {
        let Self { global, scope } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                scope.configure(cx)?;
                cx.add_modifier(modifier);
                Ok(())
            }),
        }
    }

    pub fn prefix(self, prefix: impl AsRef<str>) -> Builder<impl Scope<Error = Error>, G> {
        let Self { global, scope } = self;
        Builder {
            global,
            scope: self::scope::raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.set_prefix(prefix.as_ref())?;
                Ok(())
            }),
        }
    }

    /// Add the global-level configuration to this application.
    pub fn global(self, global: impl Global) -> Builder<S, impl Global> {
        let Self {
            global: current,
            scope,
        } = self;
        Builder {
            scope,
            global: self::global::raw(move |cx| {
                current.configure(cx);
                global.configure(cx);
            }),
        }
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(self) -> Result<App> {
        self::builder::build(self.scope, self.global)
    }
}
