//! Components for constructing HTTP applications.

mod builder;
mod error;
mod handler;
pub mod route;
pub mod scope;
pub(crate) mod service;

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

use self::builder::AppBuilderContext;
use self::scope::ScopeContext;

pub use self::error::{AppError, AppErrorKind, AppResult};
pub use self::handler::{AsyncResult, Handler, Modifier};
pub use self::route::RouteConfig;
pub use self::scope::ScopeConfig;
pub use self::service::RecognizeError;

pub use crate::route;

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

    recognizer: Recognizer,
    route_ids: Vec<IndexMap<Method, usize>>,
    config: Config,

    states: ScopedContainer,
    error_handler: Box<dyn ErrorHandler + Send + Sync + 'static>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppData")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("recognizer", &self.recognizer)
            .field("route_ids", &self.route_ids)
            .field("config", &self.config)
            .field("states", &self.states)
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
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }

    pub fn with_prefix(prefix: &str) -> AppBuilder {
        AppBuilder::with_prefix(prefix)
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

    fn get_modifier(&self, id: ModifierId) -> Option<&(dyn Modifier + Send + Sync + 'static)> {
        match id {
            ModifierId(ScopeId::Global, pos) => self.data.modifiers.get(pos).map(|m| &**m),
            ModifierId(ScopeId::Local(id), pos) => {
                self.data.scopes.get(id)?.modifiers.get(pos).map(|m| &**m)
            }
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
            .and_then(|&id| self.get_modifier(id))
    }
}

/// A builder object for constructing an instance of `App`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug, Default)]
pub struct AppBuilder<S: ScopeConfig = (), G: GlobalConfig = ()> {
    scope: S,
    global: G,
    prefix: Option<String>,
}

impl AppBuilder<(), ()> {
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: Some(prefix.to_owned()),
            ..AppBuilder::default()
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, G> AppBuilder<S, G>
where
    S: ScopeConfig,
    G: GlobalConfig,
{
    /// Adds a route into the global scope.
    pub fn route(
        self,
        route: impl RouteConfig,
    ) -> AppBuilder<impl ScopeConfig<Error = AppError>, G> {
        let Self {
            scope,
            global,
            prefix,
        } = self;
        AppBuilder {
            global,
            prefix,
            scope: self::scope::scope_config(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.route(route)?;
                Ok(())
            }),
        }
    }

    /// Creates a new scope mounted into the specified prefix onto the global scope.
    pub fn mount(
        self,
        new_scope: impl ScopeConfig,
    ) -> AppBuilder<impl ScopeConfig<Error = AppError>, G> {
        let Self {
            global,
            scope,
            prefix,
        } = self;
        AppBuilder {
            global,
            prefix,
            scope: self::scope::scope_config(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.mount(new_scope)?;
                Ok(())
            }),
        }
    }

    /// Adds a *global* variable into the application.
    pub fn state<T>(self, state: T) -> AppBuilder<impl ScopeConfig<Error = S::Error>, G>
    where
        T: Send + Sync + 'static,
    {
        let Self {
            scope,
            global,
            prefix,
        } = self;
        AppBuilder {
            global,
            prefix,
            scope: self::scope::scope_config(move |cx| {
                scope.configure(cx)?;
                cx.state(state);
                Ok(())
            }),
        }
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier<M>(self, modifier: M) -> AppBuilder<impl ScopeConfig<Error = S::Error>, G>
    where
        M: Modifier + Send + Sync + 'static,
    {
        let Self {
            global,
            scope,
            prefix,
        } = self;
        AppBuilder {
            global,
            prefix,
            scope: self::scope::scope_config(move |cx| {
                scope.configure(cx)?;
                cx.modifier(modifier);
                Ok(())
            }),
        }
    }

    /// Returns a proxy object for modifying the global-level configuration.
    pub fn config<F>(self, f: F) -> AppBuilder<S, impl GlobalConfig>
    where
        F: FnOnce(&mut Global<'_>),
    {
        let Self {
            global,
            scope,
            prefix,
        } = self;
        AppBuilder {
            scope,
            prefix,
            global: move |cx: &mut Global<'_>| {
                global.configure(cx);
                f(cx);
            },
        }
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(self) -> AppResult<App> {
        let mut cx = AppBuilderContext::default();
        self.scope
            .configure(&mut ScopeContext::new(&mut cx, ScopeId::Global))
            .map_err(Into::into)?;
        self.global.configure(&mut Global { cx: &mut cx });
        if let Some(prefix) = self.prefix {
            cx.set_prefix(ScopeId::Global, &prefix)?;
        }
        cx.finish()
    }
}

pub trait GlobalConfig {
    fn configure(self, cx: &mut Global<'_>);
}

impl GlobalConfig for () {
    fn configure(self, _: &mut Global<'_>) {}
}

impl<F> GlobalConfig for F
where
    F: FnOnce(&mut Global<'_>),
{
    fn configure(self, cx: &mut Global<'_>) {
        self(cx)
    }
}

/// A proxy object for global-level configuration.
#[derive(Debug)]
pub struct Global<'a> {
    cx: &'a mut AppBuilderContext,
}

impl<'a> Global<'a> {
    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(&mut self, enabled: bool) -> &mut Self {
        self.cx.fallback_head(enabled);
        self
    }

    /// Specifies whether to use the default `OPTIONS` handlers if it is not registered.
    ///
    /// If `enabled`, it creates the default OPTIONS handlers by collecting the registered
    /// methods from the router and then adds them to the *global* scope.
    pub fn fallback_options(&mut self, enabled: bool) -> &mut Self {
        self.cx.fallback_options(enabled);
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<E>(&mut self, error_handler: E) -> &mut Self
    where
        E: ErrorHandler + Send + Sync + 'static,
    {
        self.cx.set_error_handler(error_handler);
        self
    }
}
