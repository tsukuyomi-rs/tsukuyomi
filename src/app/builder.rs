//! Components for building an `App`.

use std::sync::Arc;
use std::{fmt, mem};

use failure::{Error, Fail};
use http::{HttpTryFrom, Method};
use indexmap::map::IndexMap;
use state::Container;

use error::handler::{DefaultErrorHandler, ErrorHandler};
use handler::Handler;
use modifier::Modifier;

use super::endpoint::Endpoint;
use super::router::{Config, Recognizer, Router, RouterEntry};
use super::scope::{self, ScopedContainer};
use super::uri::{self, Uri};
use super::{App, AppState, ScopeData, ScopeId};

/// A builder object for constructing an instance of `App`.
pub struct AppBuilder {
    endpoints: Vec<EndpointBuilder>,
    scopes: Vec<ScopeData>,
    config: Config,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    container: Container,
    container_scoped: scope::Builder,
    prefix: Option<Uri>,

    result: Result<(), Error>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppBuilder")
            .field("endpoints", &self.endpoints)
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
            endpoints: vec![],
            scopes: vec![],
            config: Default::default(),
            error_handler: None,
            modifiers: vec![],
            container: Container::new(),
            container_scoped: ScopedContainer::builder(),
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
    /// # Ok(())
    /// # }
    /// ```
    pub fn route(&mut self, config: impl RouteConfig) -> &mut Self {
        self.new_route(ScopeId::Global, config);
        self
    }

    fn new_route(&mut self, scope_id: ScopeId, config: impl RouteConfig) {
        let mut endpoint = EndpointBuilder {
            scope_id: scope_id,
            uri: Uri::new(),
            method: Method::GET,
            handler: None,
        };
        config.configure(&mut Route {
            builder: self,
            endpoint: &mut endpoint,
        });
        self.endpoints.push(endpoint);
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
        let id = ScopeId::Scope(self.scopes.len());
        let mut chain = parent
            .local_id()
            .map_or_else(Default::default, |id| self.scopes[id].chain.clone());
        chain.push(id);
        self.scopes.push(ScopeData {
            parent: parent,
            prefix: None,
            chain: chain,
            modifiers: vec![],
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

    /// Sets whether the fallback to default OPTIONS handler if not registered is enabled or not.
    ///
    /// The default value is `false`.
    pub fn fallback_options(&mut self, enabled: bool) -> &mut Self {
        self.modify(move |self_| {
            self_.config.fallback_options = enabled;
            Ok(())
        });
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
            ScopeId::Scope(id) => self.scopes[id].modifiers.push(Box::new(modifier)),
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
                self.container.set(state);
            }
            ScopeId::Scope(id) => {
                self.container_scoped.set(state, id);
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
        match Uri::from_str(prefix) {
            Ok(prefix) => match id {
                ScopeId::Scope(id) => self.scopes[id].prefix = Some(prefix),
                ScopeId::Global => self.prefix = Some(prefix),
            },
            Err(err) => self.result = Err(err.into()),
        }
    }

    /// Creates a configured `App` using the current settings.
    pub fn finish(&mut self) -> Result<App, Error> {
        let AppBuilder {
            endpoints,
            config,
            result,
            error_handler,
            modifiers,
            mut container,
            mut container_scoped,
            scopes,
            prefix,
        } = mem::replace(self, AppBuilder::new());

        result?;

        // finalize endpoints based on the created scope information.
        let endpoints: Vec<Endpoint> = endpoints
            .into_iter()
            .map(|e| e.finish(&prefix, &scopes))
            .collect::<Result<_, _>>()?;

        // create a router
        let (recognizer, entries) = build_recognizer(&endpoints)?;
        let router = Router {
            recognizer: recognizer,
            entries: entries,
            config: config,
        };

        // finalize error handler.
        let error_handler = error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::new()));

        // finalize global/scope-local storages.
        container.freeze();
        let parents: Vec<_> = scopes.iter().map(|scope| scope.parent().local_id()).collect();
        let container_scoped = container_scoped.finish(&parents[..]);

        Ok(App {
            inner: Arc::new(AppState {
                endpoints: endpoints,
                router: router,
                error_handler: error_handler,
                modifiers: modifiers,
                container,
                container_scoped,
                scopes,
            }),
        })
    }
}

fn build_recognizer(endpoints: &[Endpoint]) -> Result<(Recognizer, Vec<RouterEntry>), Error> {
    let mut entries = vec![];

    let mut collected_routes = IndexMap::new();
    for (i, endpoint) in endpoints.iter().enumerate() {
        collected_routes
            .entry(endpoint.uri.clone())
            .or_insert_with(RouterEntry::builder)
            .push(&endpoint.method, i)?;
    }

    let mut builder = Recognizer::builder();
    for (path, entry) in collected_routes {
        builder.push(path.as_ref())?;
        entries.push(entry.finish()?);
    }

    let recognizer = builder.finish();

    Ok((recognizer, entries))
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
    endpoint: &'a mut EndpointBuilder,
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
                Ok(method) => self.endpoint.method = method,
                Err(err) => self.builder.result = Err(Error::from(err.into())),
            }
        }
        self
    }

    /// Modifies the URI of this route.
    pub fn uri(&mut self, uri: &str) -> &mut Self {
        if self.builder.result.is_ok() {
            match Uri::from_str(uri) {
                Ok(uri) => self.endpoint.uri = uri,
                Err(err) => self.builder.result = Err(err),
            }
        }
        self
    }

    /// Sets a `Handler` to this route.
    pub fn handler(&mut self, handler: impl Into<Handler>) -> &mut Self {
        self.endpoint.handler = Some(handler.into());
        self
    }
}

#[derive(Debug)]
struct EndpointBuilder {
    scope_id: ScopeId,
    uri: Uri,
    method: Method,
    handler: Option<Handler>,
}

impl EndpointBuilder {
    fn finish(self, prefix: &Option<Uri>, scopes: &[ScopeData]) -> Result<Endpoint, Error> {
        let mut uris = vec![&self.uri];

        let mut current = self.scope_id.local_id();
        while let Some(scope) = current.and_then(|i| scopes.get(i)) {
            uris.extend(&scope.prefix);
            current = scope.parent.local_id();
        }
        uris.extend(prefix.as_ref());

        let uri = uri::join_all(uris.into_iter().rev());

        let handler = self.handler
            .ok_or_else(|| format_err!("default handler is not supported"))?;

        Ok(Endpoint {
            uri: uri,
            method: self.method,
            scope_id: self.scope_id,
            handler,
        })
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
    B: Into<Handler>,
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
    C: Into<Handler>,
    <Method as HttpTryFrom<B>>::Error: Fail,
{
    fn configure(self, route: &mut Route) {
        route.uri(self.0.as_ref());
        route.method(self.1);
        route.handler(self.2);
    }
}
