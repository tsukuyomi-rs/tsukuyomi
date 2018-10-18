//! Components for building an `App`.

use std::fmt;
use std::sync::Arc;

use bytes::BytesMut;
use failure::{Error, Fail};
use http::header::HeaderValue;
use http::{header, HttpTryFrom, Method, Response};
use indexmap::map::IndexMap;

use crate::error::{DefaultErrorHandler, ErrorHandler};
use crate::handler::{self, Handler};
use crate::modifier::Modifier;
use crate::recognizer::{
    uri::{self, Uri},
    Recognizer,
};

use super::scoped_map;
use super::{App, AppData, Config, ModifierId, RouteData, RouteId, ScopeData, ScopeId};

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    routes: Vec<RouteBuilder>,
    scopes: Vec<ScopeBuilder>,
    config: Config,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    globals: scoped_map::Builder,
    prefix: Option<Uri>,

    result: Result<(), Error>,
}

struct ScopeBuilder {
    id: ScopeId,
    parent: ScopeId,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    prefix: Option<Uri>,
    chain: Vec<ScopeId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeBuilder")
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .field("chain", &self.chain)
            .finish()
    }
}

struct RouteBuilder {
    scope_id: ScopeId,
    uri: Uri,
    method: Method,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    handler: Option<Box<dyn Handler + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteBuilder")
            .field("scope_id", &self.scope_id)
            .field("uri", &self.uri)
            .field("method", &self.method)
            .finish()
    }
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppBuilder")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("config", &self.config)
            .field("prefix", &self.prefix)
            .field("result", &self.result)
            .finish()
    }
}

impl AppBuilder {
    pub(super) fn new() -> AppBuilder {
        AppBuilder {
            routes: vec![],
            scopes: vec![],
            config: Default::default(),
            error_handler: None,
            modifiers: vec![],
            globals: Default::default(),
            prefix: None,

            result: Ok(()),
        }
    }

    fn modify(&mut self, f: impl FnOnce(&mut Self) -> Result<(), Error>) {
        if self.result.is_ok() {
            self.result = f(self);
        }
    }

    /// Adds a route into the global scope.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tsukuyomi;
    /// # extern crate http;
    /// # use tsukuyomi::app::App;
    /// use tsukuyomi::app::builder::Route;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handle;
    /// # use http::Method;
    ///
    /// fn handler(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    /// fn submit(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    ///
    /// # fn main() -> tsukuyomi::AppResult<()> {
    /// let app = App::builder()
    ///     .route(("/", handler))
    ///     .route(("/", Method::POST, submit))
    ///     .route(|r: &mut Route| {
    ///         r.uri("/submit");
    ///         r.method(Method::POST);
    ///         r.handler(submit);
    ///     })
    ///     .finish()?;
    /// # drop(app);
    /// # Ok(())
    /// # }
    /// ```
    pub fn route(mut self, config: impl RouteConfig) -> Self {
        self.new_route(ScopeId::Global, config);
        self
    }

    fn new_route(&mut self, scope_id: ScopeId, config: impl RouteConfig) {
        let mut route = RouteBuilder {
            scope_id,
            uri: Uri::root(),
            method: Method::GET,
            modifiers: vec![],
            handler: None,
        };
        config.configure(&mut Route {
            builder: self,
            route: &mut route,
        });
        self.routes.push(route);
    }

    /// Creates a new scope with the provided configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::app::App;
    /// use tsukuyomi::app::builder::Scope;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handle;
    ///
    /// fn get_post(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    /// fn add_post(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    ///
    /// # fn main() -> tsukuyomi::AppResult<()> {
    /// let app = App::builder()
    ///     .scope(|s: &mut Scope| {
    ///         s.prefix("/api/v1");
    ///         s.route(("/posts/:id", get_post));
    ///         s.route(("/posts", "POST", add_post));
    ///     })
    ///     .finish()?;
    /// # drop(app);
    /// # Ok(())
    /// # }
    /// ```
    pub fn scope(mut self, config: impl ScopeConfig) -> Self {
        self.new_scope(ScopeId::Global, config);
        self
    }

    fn new_scope(&mut self, parent: ScopeId, config: impl ScopeConfig) {
        let id = ScopeId::Local(self.scopes.len());
        let mut chain = parent
            .local_id()
            .map_or_else(Default::default, |id| self.scopes[id].chain.clone());
        chain.push(id);
        self.scopes.push(ScopeBuilder {
            id,
            parent,
            prefix: None,
            modifiers: vec![],
            chain,
        });

        config.configure(&mut Scope { builder: self, id });
    }

    /// Create a new scope mounted to the certain URI.
    ///
    /// This method is a shortcut of `AppBuilder::scope(Mount(prefix, f))`
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::app::App;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handle;
    /// fn get_post(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    /// fn add_post(_: &mut Input) -> Handle {
    ///     // ...
    /// # unimplemented!()
    /// }
    ///
    /// # fn main() -> tsukuyomi::AppResult<()> {
    /// let app = App::builder()
    ///     .mount("/api/v1", |s| {
    ///         s.route(("/posts/:id", get_post));
    ///         s.route(("/posts", "POST", add_post));
    ///     })
    ///     .finish()?;
    /// # drop(app);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn mount(self, prefix: &str, f: impl FnOnce(&mut Scope<'_>)) -> Self {
        self.scope(Mount(prefix, f))
    }

    /// Sets whether the fallback to GET if the handler for HEAD is not registered is enabled or not.
    ///
    /// The default value is `true`.
    pub fn fallback_head(mut self, enabled: bool) -> Self {
        self.modify(move |self_| {
            self_.config.fallback_head = enabled;
            Ok(())
        });
        self
    }

    /// Specifies whether to use the default OPTIONS handlers.
    ///
    /// If `enabled`, it creates the default OPTIONS handlers by collecting the registered
    /// methods from the router and then adds them to the global scope.
    pub fn default_options(mut self, enabled: bool) -> Self {
        self.modify(move |self_| {
            self_.config.fallback_options = enabled;
            Ok(())
        });
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler(
        mut self,
        error_handler: impl ErrorHandler + Send + Sync + 'static,
    ) -> Self {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier(mut self, modifier: impl Modifier + Send + Sync + 'static) -> Self {
        self.add_modifier(ScopeId::Global, modifier);
        self
    }

    fn add_modifier(&mut self, id: ScopeId, modifier: impl Modifier + Send + Sync + 'static) {
        match id {
            ScopeId::Global => self.modifiers.push(Box::new(modifier)),
            ScopeId::Local(id) => self.scopes[id].modifiers.push(Box::new(modifier)),
        }
    }

    /// Sets a value of `T` to the global storage.
    pub fn set<T>(mut self, value: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.set_value(value, ScopeId::Global);
        self
    }

    fn set_value<T>(&mut self, value: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        self.globals.set(value, id);
    }

    /// Sets the prefix of URIs.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.set_prefix(prefix, ScopeId::Global);
        self
    }

    fn set_prefix(&mut self, prefix: &str, id: ScopeId) {
        if self.result.is_err() {
            return;
        }
        match prefix.parse() {
            Ok(prefix) => match id {
                ScopeId::Local(id) => self.scopes[id].prefix = Some(prefix),
                ScopeId::Global => self.prefix = Some(prefix),
            },
            Err(err) => self.result = Err(err),
        }
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(self) -> Result<App, Error> {
        let AppBuilder {
            routes,
            config,
            result,
            error_handler,
            modifiers,
            globals,
            scopes,
            prefix,
        } = self;

        result?;

        // finalize endpoints based on the created scope information.
        let mut routes: Vec<RouteData> = routes
            .into_iter()
            .enumerate()
            .map(|(route_id, route)| -> Result<RouteData, Error> {
                // build absolute URI.
                let mut uris = vec![&route.uri];
                let mut current = route.scope_id.local_id();
                while let Some(scope) = current.and_then(|i| scopes.get(i)) {
                    uris.extend(&scope.prefix);
                    current = scope.parent.local_id();
                }
                uris.extend(prefix.as_ref());
                let uri = uri::join_all(uris.into_iter().rev())?;

                let handler = route
                    .handler
                    .ok_or_else(|| format_err!("default handler is not supported"))?;

                // calculate the modifier identifiers.
                let mut modifier_ids: Vec<_> = (0..modifiers.len())
                    .map(|i| ModifierId::Scope(ScopeId::Global, i))
                    .collect();
                if let Some(scope) = route.scope_id.local_id().and_then(|id| scopes.get(id)) {
                    for (id, scope) in scope.chain.iter().filter_map(|&id| {
                        id.local_id()
                            .and_then(|id| scopes.get(id).map(|scope| (id, scope)))
                    }) {
                        modifier_ids.extend(
                            (0..scope.modifiers.len())
                                .map(|pos| ModifierId::Scope(ScopeId::Local(id), pos)),
                        );
                    }
                }
                modifier_ids
                    .extend((0..route.modifiers.len()).map(|pos| ModifierId::Route(route_id, pos)));

                let id = RouteId(route.scope_id, route_id);

                Ok(RouteData {
                    id,
                    uri,
                    method: route.method,
                    modifiers: route.modifiers,
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

                if methods.contains_key(&route.method) {
                    bail!("Adding routes with duplicate URI and method is currenly not supported.");
                }

                methods.insert(route.method.clone(), i);
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
                            method: Method::OPTIONS,
                            modifiers: vec![],
                            handler: default_options_handler(m),
                            modifier_ids: (0..modifiers.len())
                                .map(|i| ModifierId::Scope(ScopeId::Global, i))
                                .collect(),
                        });
                        id
                    });
                }

                recognizer.add_route(uri)?;
                route_ids.push(methods);
            }

            (recognizer, route_ids)
        };

        // finalize error handler.
        let error_handler =
            error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::default()));

        // finalize global/scope-local storages.
        let parents: Vec<_> = scopes.iter().map(|scope| scope.parent).collect();
        let globals = globals.finish(&parents[..]);

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
                globals,
            }),
        })
    }
}

// ==== Scope ====

/// A proxy object for configuration of a scope.
#[derive(Debug)]
pub struct Scope<'a> {
    builder: &'a mut AppBuilder,
    id: ScopeId,
}

impl<'a> Scope<'a> {
    /// Adds a route into the current scope, with the provided configuration.
    pub fn route(&mut self, config: impl RouteConfig) -> &mut Self {
        self.builder.new_route(self.id, config);
        self
    }

    /// Create a new sub-scope with the provided configuration.
    pub fn scope(&mut self, config: impl ScopeConfig) -> &mut Self {
        self.builder.new_scope(self.id, config);
        self
    }

    /// Create a new scope mounted to the certain URI.
    ///
    /// This method is a shortcut of `Scope::scope(Mount(prefix, f))`.
    #[inline(always)]
    pub fn mount(&mut self, prefix: &str, f: impl FnOnce(&mut Scope<'_>)) -> &mut Self {
        self.scope(Mount(prefix, f))
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set<T>(&mut self, value: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.builder.set_value(value, self.id);
        self
    }

    /// Modifies the prefix URI of current scope.
    pub fn prefix(&mut self, prefix: &str) -> &mut Self {
        self.builder.set_prefix(prefix, self.id);
        self
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier(&mut self, modifier: impl Modifier + Send + Sync + 'static) -> &mut Self {
        self.builder.add_modifier(self.id, modifier);
        self
    }
}

/// Trait representing a set of configuration for setting a scope.
pub trait ScopeConfig {
    /// Applies this configuration to the provided `Scope`.
    fn configure(self, scope: &mut Scope<'_>);
}

impl<F> ScopeConfig for F
where
    F: FnOnce(&mut Scope<'_>),
{
    fn configure(self, scope: &mut Scope<'_>) {
        self(scope)
    }
}

/// A helper struct for instantiating a `ScopeConfig` from a prefix URI and a function.
#[derive(Debug)]
pub struct Mount<P, F>(pub P, pub F);

impl<P, F> ScopeConfig for Mount<P, F>
where
    P: AsRef<str>,
    F: FnOnce(&mut Scope<'_>),
{
    #[inline(always)]
    fn configure(self, scope: &mut Scope<'_>) {
        scope.prefix(self.0.as_ref());
        (self.1)(scope);
    }
}

// ==== Route ====

/// A proxy object for creating an endpoint.
#[derive(Debug)]
pub struct Route<'a> {
    builder: &'a mut AppBuilder,
    route: &'a mut RouteBuilder,
}

impl<'a> Route<'a> {
    /// Modifies the HTTP method of this route.
    pub fn method<M>(&mut self, method: M) -> &mut Self
    where
        Method: HttpTryFrom<M>,
        <Method as HttpTryFrom<M>>::Error: Fail,
    {
        if self.builder.result.is_ok() {
            match Method::try_from(method) {
                Ok(method) => self.route.method = method,
                Err(err) => self.builder.result = Err(Error::from(err.into())),
            }
        }
        self
    }

    /// Modifies the URI of this route.
    pub fn uri(&mut self, uri: &str) -> &mut Self {
        if self.builder.result.is_ok() {
            match uri.parse() {
                Ok(uri) => self.route.uri = uri,
                Err(err) => self.builder.result = Err(err),
            }
        }
        self
    }

    /// Register a `Modifier` to this route.
    pub fn modifier(&mut self, modifier: impl Modifier + Send + Sync + 'static) -> &mut Self {
        self.route.modifiers.push(Box::new(modifier));
        self
    }

    /// Sets a `Handler` to this route.
    pub fn handler(&mut self, handler: impl Handler + Send + Sync + 'static) -> &mut Self {
        self.route.handler = Some(Box::new(handler));
        self
    }
}

/// Trait representing a set of configuration for setting a route.
pub trait RouteConfig {
    /// Applies this configuration to the provided `Route`.
    fn configure(self, route: &mut Route<'_>);
}

impl<F> RouteConfig for F
where
    F: FnOnce(&mut Route<'_>),
{
    fn configure(self, route: &mut Route<'_>) {
        self(route)
    }
}

impl<A, B> RouteConfig for (A, B)
where
    A: AsRef<str>,
    B: Handler + Send + Sync + 'static,
{
    fn configure(self, route: &mut Route<'_>) {
        route.uri(self.0.as_ref());
        route.handler(self.1);
    }
}

impl<A, B, C> RouteConfig for (A, B, C)
where
    A: AsRef<str>,
    Method: HttpTryFrom<B>,
    C: Handler + Send + Sync + 'static,
    <Method as HttpTryFrom<B>>::Error: Fail,
{
    fn configure(self, route: &mut Route<'_>) {
        route.uri(self.0.as_ref());
        route.method(self.1);
        route.handler(self.2);
    }
}

// ====

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

    Box::new(handler::wrap_ready(move |_| {
        let mut response = Response::new(());
        response
            .headers_mut()
            .insert(header::ALLOW, allowed_methods.clone());
        response
    }))
}
