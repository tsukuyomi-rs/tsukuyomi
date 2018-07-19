//! Components for building an `App`.

use std::sync::Arc;
use std::{fmt, mem};

use bytes::BytesMut;
use failure::{Error, Fail};
use http::header::HeaderValue;
use http::{header, HttpTryFrom, Method, Response};
use indexmap::map::IndexMap;
use state::Container;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use handler::{Handle, Handler};
use input::Input;
use modifier::Modifier;

use super::container::{self, ScopedContainer};
use super::recognizer::Recognizer;
use super::uri::{self, Uri};
use super::{App, AppState, Config, ModifierId, RouteData, RouteId, ScopeData, ScopeId};

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    routes: Vec<RouteBuilder>,
    scopes: Vec<ScopeBuilder>,
    config: Config,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    state: Container,
    scoped_state: container::Builder,
    prefix: Option<Uri>,
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    options_handler: Option<Box<dyn FnMut(Vec<Method>) -> Box<dyn Handler + Send + Sync + 'static>>>,

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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RouteBuilder")
            .field("scope_id", &self.scope_id)
            .field("uri", &self.uri)
            .field("method", &self.method)
            .finish()
    }
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
            state: Container::new(),
            scoped_state: ScopedContainer::builder(),
            prefix: None,
            options_handler: Some(Box::new(default_options_handler)),

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
    /// # Ok(())
    /// # }
    /// ```
    pub fn route(&mut self, config: impl RouteConfig) -> &mut Self {
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
    /// # Ok(())
    /// # }
    /// ```
    pub fn scope(&mut self, config: impl ScopeConfig) -> &mut Self {
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
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn mount(&mut self, prefix: &str, f: impl FnOnce(&mut Scope)) -> &mut Self {
        self.scope(Mount(prefix, f))
    }

    /// Sets whether the fallback to GET if the handler for HEAD is not registered is enabled or not.
    ///
    /// The default value is `true`.
    pub fn fallback_head(&mut self, enabled: bool) -> &mut Self {
        self.modify(move |self_| {
            self_.config.fallback_head = enabled;
            Ok(())
        });
        self
    }

    /// Specifies whether to use the fallback OPTIONS handlers if the handler is not set.
    ///
    /// If a function is provided, the builder creates the instances of handler function by using the provided
    /// function for each registered route, and then specifies them to each route as OPTIONS handlers.
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    pub fn default_options(
        &mut self,
        handler: Option<Box<dyn FnMut(Vec<Method>) -> Box<dyn Handler + Send + Sync + 'static> + 'static>>,
    ) -> &mut Self {
        self.options_handler = handler;
        self
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler<H>(&mut self, error_handler: H) -> &mut Self
    where
        H: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
        self
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier(&mut self, modifier: impl Modifier + Send + Sync + 'static) -> &mut Self {
        self.modifiers.push(Box::new(modifier));
        self
    }

    fn add_modifier(&mut self, id: ScopeId, modifier: impl Modifier + Send + Sync + 'static) {
        match id {
            ScopeId::Global => self.modifiers.push(Box::new(modifier)),
            ScopeId::Local(id) => self.scopes[id].modifiers.push(Box::new(modifier)),
        }
    }

    /// Sets a value of `T` to the global storage.
    pub fn set<T>(&mut self, state: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.set_value(state, ScopeId::Global);
        self
    }

    fn set_value<T>(&mut self, state: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        match id {
            ScopeId::Global => {
                self.state.set(state);
            }
            ScopeId::Local(id) => {
                self.scoped_state.set(state, id);
            }
        }
    }

    /// Sets the prefix of URIs.
    pub fn prefix(&mut self, prefix: &str) -> &mut Self {
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
    pub fn finish(&mut self) -> Result<App, Error> {
        let AppBuilder {
            routes,
            config,
            result,
            error_handler,
            modifiers,
            mut state,
            mut scoped_state,
            scopes,
            prefix,
            mut options_handler,
        } = mem::replace(self, AppBuilder::new());

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
                let uri = uri::join_all(uris.into_iter().rev());

                let handler = route
                    .handler
                    .ok_or_else(|| format_err!("default handler is not supported"))?;

                // calculate the modifier identifiers.
                let mut modifier_ids: Vec<_> = (0..modifiers.len()).map(ModifierId::Global).collect();
                if let Some(scope) = route.scope_id.local_id().and_then(|id| scopes.get(id)) {
                    for (id, scope) in scope
                        .chain
                        .iter()
                        .filter_map(|&id| id.local_id().and_then(|id| scopes.get(id).map(|scope| (id, scope))))
                    {
                        modifier_ids.extend((0..scope.modifiers.len()).map(|pos| ModifierId::Scope(id, pos)));
                    }
                }
                modifier_ids.extend((0..route.modifiers.len()).map(|pos| ModifierId::Route(route_id, pos)));

                let id = RouteId(route.scope_id, route_id);

                Ok(RouteData {
                    id,
                    uri,
                    method: route.method,
                    modifiers: route.modifiers,
                    handler,
                    modifier_ids,
                })
            })
            .collect::<Result<_, _>>()?;

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

            let mut recognizer = Recognizer::builder();
            let mut route_ids = vec![];
            for (uri, mut methods) in collected_routes {
                if let Some(ref mut f) = options_handler {
                    let m = methods.keys().cloned().chain(Some(Method::OPTIONS)).collect();
                    methods.entry(Method::OPTIONS).or_insert_with(|| {
                        let id = routes.len();
                        routes.push(RouteData {
                            id: RouteId(ScopeId::Global, id),
                            uri: uri.clone(),
                            method: Method::OPTIONS,
                            modifiers: vec![],
                            handler: (f)(m),
                            modifier_ids: (0..modifiers.len()).map(ModifierId::Global).collect(),
                        });
                        id
                    });
                }

                recognizer.push(uri.as_ref())?;
                route_ids.push(methods);
            }

            (recognizer.finish(), route_ids)
        };

        // finalize error handler.
        let error_handler = error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::new()));

        // finalize global/scope-local storages.
        state.freeze();
        let parents: Vec<_> = scopes.iter().map(|scope| scope.parent.local_id()).collect();
        let scoped_state = scoped_state.finish(&parents[..]);

        let scopes = scopes
            .into_iter()
            .map(|scope| ScopeData {
                id: scope.id,
                parent: scope.parent,
                prefix: scope.prefix,
                modifiers: scope.modifiers,
            })
            .collect();

        Ok(App {
            inner: Arc::new(AppState {
                routes,
                scopes,
                recognizer,
                route_ids,
                config,
                error_handler,
                modifiers,
                state,
                scoped_state,
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
    pub fn mount(&mut self, prefix: &str, f: impl FnOnce(&mut Scope)) -> &mut Self {
        self.scope(Mount(prefix, f))
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set<T>(&mut self, state: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.builder.set_value(state, self.id);
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
    fn configure(self, scope: &mut Scope);
}

impl<F> ScopeConfig for F
where
    F: FnOnce(&mut Scope),
{
    fn configure(self, scope: &mut Scope) {
        self(scope)
    }
}

/// A helper struct for instantiating a `ScopeConfig` from a prefix URI and a function.
#[derive(Debug)]
pub struct Mount<P, F>(pub P, pub F);

impl<P, F> ScopeConfig for Mount<P, F>
where
    P: AsRef<str>,
    F: FnOnce(&mut Scope),
{
    #[inline(always)]
    fn configure(self, scope: &mut Scope) {
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
    fn configure(self, route: &mut Route);
}

impl<F> RouteConfig for F
where
    F: FnOnce(&mut Route),
{
    fn configure(self, route: &mut Route) {
        self(route)
    }
}

impl<A, B> RouteConfig for (A, B)
where
    A: AsRef<str>,
    B: Handler + Send + Sync + 'static,
{
    fn configure(self, route: &mut Route) {
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
    fn configure(self, route: &mut Route) {
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

    Box::new(move |_: &mut Input| -> Handle {
        let mut response = Response::new(());
        response.headers_mut().insert(header::ALLOW, allowed_methods.clone());
        Handle::ready(Ok(response.into()))
    })
}
