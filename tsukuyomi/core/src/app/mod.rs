//! Components for constructing HTTP applications.

mod container;
pub mod route;
pub(crate) mod scope;
mod service;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use bytes::BytesMut;
use http::header::HeaderValue;
use http::{header, Method, Response};
use indexmap::IndexMap;

use crate::error::handler::{DefaultErrorHandler, ErrorHandler};
use crate::handler;
use crate::handler::{Handle, Handler};
use crate::modifier::Modifier;
use crate::output::ResponseBody;
use crate::recognizer::uri;
use crate::recognizer::uri::Uri;
use crate::recognizer::Recognizer;

use self::container::Builder as ScopedContainerBuilder;
use self::container::ScopedContainer;
use self::route::{RouteData, RouteId};
use self::scope::{ScopeBuilder, ScopeData, ScopeId};

pub use self::route::Route;
pub use self::scope::Scope;
pub use self::service::RecognizeError;

/// Configure an instance of `App` using the specified function.
///
/// This function is a shortcut of `App::build`.
#[inline]
pub fn app<F>(f: F) -> AppResult<App>
where
    F: FnOnce(&mut Scope<'_>),
{
    App::build(f)
}

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub type AppResult<T> = std::result::Result<T, AppError>;

/// An error type which will be thrown from `AppBuilder`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug, failure::Fail)]
#[fail(display = "{}", inner)]
pub struct AppError {
    inner: failure::Error,
}

impl AppError {
    pub(crate) fn from_failure(err: impl Into<failure::Error>) -> Self {
        Self { inner: err.into() }
    }
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

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    data: Arc<AppData>,
}

impl App {
    /// Configure an instance of `App` using the specified function.
    pub fn build<F>(f: F) -> AppResult<Self>
    where
        F: FnOnce(&mut Scope<'_>),
    {
        let mut builder = AppBuilder::new();
        f(&mut Scope::new(&mut builder, ScopeId::Global));
        builder.finish()
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
}

/// A builder object for constructing an instance of `App`.
struct AppBuilder {
    routes: Vec<(ScopeId, Route)>,
    scopes: Vec<ScopeBuilder>,
    config: Config,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: ScopedContainerBuilder,
    result: AppResult<()>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppBuilder")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("config", &self.config)
            .field("result", &self.result)
            .finish()
    }
}

impl AppBuilder {
    pub(super) fn new() -> Self {
        Self {
            routes: vec![],
            scopes: vec![],
            config: Config::default(),
            error_handler: None,
            modifiers: vec![],
            states: ScopedContainerBuilder::default(),
            result: Ok(()),
        }
    }

    fn modify(&mut self, f: impl FnOnce(&mut Self) -> AppResult<()>) {
        if self.result.is_ok() {
            self.result = f(self);
        }
    }

    pub(super) fn new_route(&mut self, scope_id: ScopeId, route: Route) {
        if self.result.is_ok() {
            self.routes.push((scope_id, route));
        }
    }

    pub(super) fn new_scope<F>(&mut self, parent: ScopeId, prefix: &str, f: F)
    where
        F: FnOnce(&mut Scope<'_>),
    {
        if self.result.is_err() {
            return;
        }

        let prefix = match prefix.parse() {
            Ok(prefix) => prefix,
            Err(err) => {
                self.mark_error(err);
                return;
            }
        };

        let id = ScopeId::Local(self.scopes.len());
        let mut chain = parent
            .local_id()
            .map_or_else(Default::default, |id| self.scopes[id].chain.clone());
        chain.push(id);
        self.scopes.push(ScopeBuilder {
            id,
            parent,
            prefix,
            modifiers: vec![],
            chain,
        });

        f(&mut Scope::new(self, id));
    }

    pub(super) fn add_modifier(
        &mut self,
        id: ScopeId,
        modifier: impl Modifier + Send + Sync + 'static,
    ) {
        if self.result.is_ok() {
            match id {
                ScopeId::Global => self.modifiers.push(Box::new(modifier)),
                ScopeId::Local(id) => self.scopes[id].modifiers.push(Box::new(modifier)),
            }
        }
    }

    pub(super) fn set_state<T>(&mut self, value: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        if self.result.is_ok() {
            self.states.set(value, id);
        }
    }

    pub(super) fn mark_error(&mut self, err: impl Into<failure::Error>) {
        self.result = Err(AppError::from_failure(err));
    }

    /// Creates a configured `App` using the current settings.
    pub(crate) fn finish(&mut self) -> AppResult<App> {
        let Self {
            routes,
            config,
            result,
            error_handler,
            modifiers,
            states,
            scopes,
        } = std::mem::replace(self, Self::new());
        self.mark_error(failure::format_err!("The build has already used"));

        if let Err(err) = result {
            log::debug!("error before building App: {}", err);
            return Err(err);
        }

        // finalize endpoints based on the created scope information.
        let mut routes: Vec<RouteData> = routes
            .into_iter()
            .enumerate()
            .map(|(route_id, (scope_id, route))| -> AppResult<RouteData> {
                let route = route.inner?;
                // build absolute URI.
                let mut uris = vec![&route.uri];
                let mut current = scope_id.local_id();
                while let Some(scope) = current.and_then(|i| scopes.get(i)) {
                    uris.extend(Some(&scope.prefix));
                    current = scope.parent.local_id();
                }
                let uri = uri::join_all(uris.into_iter().rev()).map_err(AppError::from_failure)?;

                let handler = route.handler;

                // calculate the modifier identifiers.
                let mut modifier_ids: Vec<_> = (0..modifiers.len())
                    .map(|i| ModifierId(ScopeId::Global, i))
                    .collect();
                if let Some(scope) = scope_id.local_id().and_then(|id| scopes.get(id)) {
                    for (id, scope) in scope.chain.iter().filter_map(|&id| {
                        id.local_id()
                            .and_then(|id| scopes.get(id).map(|scope| (id, scope)))
                    }) {
                        modifier_ids.extend(
                            (0..scope.modifiers.len())
                                .map(|pos| ModifierId(ScopeId::Local(id), pos)),
                        );
                    }
                }

                let id = RouteId(scope_id, route_id);

                let mut methods = route.methods;
                if methods.is_empty() {
                    methods.insert(Method::GET);
                }

                Ok(RouteData {
                    id,
                    uri,
                    methods,
                    handler,
                    modifier_ids,
                })
            }).collect::<Result<_, _>>()?;

        // create a router
        let (recognizer, route_ids) = {
            let mut collected_routes = IndexMap::<Uri, IndexMap<Method, usize>>::new();
            for (i, route) in routes.iter().enumerate() {
                let methods = collected_routes
                    .entry(route.uri.clone())
                    .or_insert_with(IndexMap::<Method, usize>::new);

                for method in &route.methods {
                    if methods.contains_key(method) {
                        return Err(AppError::from_failure(failure::format_err!(
                            "Adding routes with duplicate URI and method is currenly not supported. \
                            (uri={}, method={})",
                            route.uri,
                            method
                        )));
                    }

                    methods.insert(method.clone(), i);
                }
            }

            log::debug!("collected routes:");
            for (uri, methods) in &collected_routes {
                log::debug!(" - {} {:?}", uri, methods.keys().collect::<Vec<_>>());
            }

            let mut recognizer = Recognizer::default();
            let mut route_ids = vec![];
            for (uri, mut methods) in collected_routes {
                if config.fallback_options {
                    let m = methods
                        .keys()
                        .cloned()
                        .chain(Some(Method::OPTIONS))
                        .collect();
                    methods.entry(Method::OPTIONS).or_insert_with(|| {
                        let id = routes.len();
                        routes.push(RouteData {
                            id: RouteId(ScopeId::Global, id),
                            uri: uri.clone(),
                            methods: vec![Method::OPTIONS].into_iter().collect(),
                            handler: default_options_handler(m),
                            modifier_ids: (0..modifiers.len())
                                .map(|i| ModifierId(ScopeId::Global, i))
                                .collect(),
                        });
                        id
                    });
                }

                recognizer.add_route(uri).map_err(AppError::from_failure)?;
                route_ids.push(methods);
            }

            (recognizer, route_ids)
        };

        // finalize error handler.
        let error_handler =
            error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::default()));

        // finalize global/scope-local storages.
        let parents: Vec<_> = scopes.iter().map(|scope| scope.parent).collect();
        let states = states.finish(&parents[..]);

        let scopes = scopes
            .into_iter()
            .map(|scope| ScopeData {
                id: scope.id,
                parent: scope.parent,
                prefix: scope.prefix,
                modifiers: scope.modifiers,
            }).collect();

        Ok(App {
            data: Arc::new(AppData {
                routes,
                scopes,
                recognizer,
                route_ids,
                config,
                error_handler,
                modifiers,
                states,
            }),
        })
    }
}

/// A proxy object for global-level configuration.
#[derive(Debug)]
pub struct Global<'a> {
    builder: &'a mut AppBuilder,
}

impl<'a> Global<'a> {
    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(&mut self, enabled: bool) -> &mut Self {
        self.builder.modify(move |self_| {
            self_.config.fallback_head = enabled;
            Ok(())
        });
        self
    }

    /// Specifies whether to use the default `OPTIONS` handlers if it is not registered.
    ///
    /// If `enabled`, it creates the default OPTIONS handlers by collecting the registered
    /// methods from the router and then adds them to the *global* scope.
    pub fn fallback_options(&mut self, enabled: bool) -> &mut Self {
        self.builder.modify(move |self_| {
            self_.config.fallback_options = enabled;
            Ok(())
        });
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<E>(&mut self, error_handler: E) -> &mut Self
    where
        E: ErrorHandler + Send + Sync + 'static,
    {
        if self.builder.result.is_ok() {
            self.builder.error_handler = Some(Box::new(error_handler));
        }
        self
    }
}

fn default_options_handler(methods: Vec<Method>) -> Box<dyn Handler + Send + Sync + 'static> {
    let allowed_methods = {
        let bytes = methods
            .into_iter()
            .enumerate()
            .fold(BytesMut::new(), |mut acc, (i, m)| {
                if i > 0 {
                    acc.extend_from_slice(b", ");
                }
                acc.extend_from_slice(m.as_str().as_bytes());
                acc
            });
        unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
    };

    Box::new(handler::raw(move |_| {
        let mut response = Response::new(ResponseBody::empty());
        response
            .headers_mut()
            .insert(header::ALLOW, allowed_methods.clone());
        Handle::ready(Ok(response))
    }))
}
