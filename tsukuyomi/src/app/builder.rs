use {
    super::{
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
        route::{Context as RouteContext, Route},
        scope::{Context as ScopeContext, Scope},
        App, AppData, Config, EndpointData, EndpointId, RouteData, RouteId, ScopeData,
    },
    bytes::BytesMut,
    crate::{
        modifier::Modifier,
        recognizer::Recognizer,
        scoped_map::{Builder as ScopedContainerBuilder, ScopeId},
        uri::Uri,
    },
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

/// A builder object for constructing an instance of `App`.
#[derive(Default)]
pub struct Builder<S: Scope = ()> {
    scope: super::scope::Builder<S>,
    config: Config,
}

impl<S> fmt::Debug for Builder<S>
where
    S: Scope + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("scope", &self.scope)
            .field("config", &self.config)
            .finish()
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    /// Adds a route into the global scope.
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>> {
        Builder {
            config: self.config,
            scope: self.scope.route(route),
        }
    }

    /// Creates a new scope onto the global scope using the specified `Scope`.
    pub fn mount(self, new_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            config: self.config,
            scope: self.scope.mount(new_scope),
        }
    }

    /// Merges the specified `Scope` into the global scope, *without* creating a new scope.
    pub fn with(self, scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            config: self.config,
            scope: self.scope.with(scope),
        }
    }

    /// Adds a *global* variable into the application.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>>
    where
        T: Send + Sync + 'static,
    {
        Builder {
            config: self.config,
            scope: self.scope.state(state),
        }
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier<M>(self, modifier: M) -> Builder<impl Scope<Error = S::Error>>
    where
        M: Modifier + Send + Sync + 'static,
    {
        Builder {
            config: self.config,
            scope: self.scope.modifier(modifier),
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Builder<impl Scope<Error = S::Error>>
    where
        F: Fallback + Send + Sync + 'static,
    {
        self.state(FallbackInstance::from(fallback))
    }

    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = S::Error>> {
        Builder {
            config: self.config,
            scope: self.scope.prefix(prefix),
        }
    }

    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(mut self, enabled: bool) -> Builder<S> {
        self.config.fallback_head = enabled;
        self
    }

    /// Creates an `App` using the current configuration.
    pub fn build(self) -> Result<App> {
        build(self.scope, self.config)
    }

    /// Creates a builder of HTTP server using the current configuration.
    pub fn build_server(self) -> Result<crate::server::Server<App>> {
        self.build().map(crate::server::Server::new)
    }
}

fn build(scope: impl Scope, config: Config) -> Result<App> {
    let mut cx = AppContext {
        endpoints: IndexMap::new(),
        routes: vec![],
        scopes: vec![],
        global_scope: ScopeData {
            id: ScopeId::Global,
            parents: vec![],
            prefix: None,
            modifiers: vec![],
        },
        states: ScopedContainerBuilder::default(),
    };

    scope
        .configure(&mut ScopeContext::new(&mut cx, ScopeId::Global))
        .map_err(Into::into)?;

    let AppContext {
        mut endpoints,
        routes,
        scopes,
        global_scope,
        states,
    } = cx;

    // finalize global/scope-local storages.
    let parents: Vec<_> = scopes
        .iter()
        .map(|scope| *scope.parents.last().expect("no parent"))
        .collect();
    let states = states.finish(&parents[..]);

    // create a route recognizer.
    let mut recognizer = Recognizer::default();
    for uri in endpoints.keys().cloned() {
        recognizer.add_route(uri)?;
    }

    for endpoint in endpoints.values_mut() {
        endpoint.allowed_methods_value = {
            let allowed_methods: IndexSet<_> = endpoint
                .route_ids
                .keys()
                .chain(Some(&Method::OPTIONS))
                .collect();
            let bytes =
                allowed_methods
                    .iter()
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
    }

    Ok(App {
        data: Arc::new(AppData {
            routes,
            scopes,
            global_scope,
            recognizer,
            endpoints,
            config,
            states,
        }),
    })
}

#[derive(Debug)]
pub struct AppContext {
    routes: Vec<RouteData>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,
    endpoints: IndexMap<Uri, EndpointData>,
    states: ScopedContainerBuilder,
}

impl AppContext {
    fn build_absolute_uri(&self, scope_id: ScopeId, suffix: &Uri) -> Result<Uri> {
        let mut uris = vec![];
        let scope = match scope_id {
            ScopeId::Global => &self.global_scope,
            ScopeId::Local(i) => &self.scopes[i],
        };
        uris.extend(scope.prefix.as_ref());
        for &scope_id in scope.parents.iter().rev() {
            let scope = match scope_id {
                ScopeId::Global => &self.global_scope,
                ScopeId::Local(i) => &self.scopes[i],
            };
            uris.extend(scope.prefix.as_ref());
        }
        crate::uri::join_all(uris.into_iter().rev().chain(Some(suffix))).map_err(Into::into)
    }

    pub(super) fn new_route(&mut self, scope_id: ScopeId, route: impl Route) -> Result<()> {
        let mut cx = RouteContext {
            uri: Uri::root(),
            methods: None,
            handler: None,
        };
        route.configure(&mut cx);

        // build absolute URI.
        let uri = self.build_absolute_uri(scope_id, &cx.uri)?;

        let endpoint = {
            let pos = self.endpoints.len();
            self.endpoints
                .entry(uri.clone())
                .or_insert_with(|| EndpointData {
                    id: EndpointId(scope_id, pos),
                    uri: uri.clone(),
                    route_ids: IndexMap::new(),
                    allowed_methods_value: HeaderValue::from_static(""),
                })
        };

        if scope_id != endpoint.id.0 {
            return Err(Error::from(failure::format_err!(
                "all routes with the same URI belong to the same scope"
            )));
        }

        let mut methods = cx.methods.unwrap_or_default();
        if methods.is_empty() {
            methods.insert(Method::GET);
        }

        if uri.is_asterisk() {
            if !methods.contains(&Method::OPTIONS) {
                return Err(failure::format_err!(
                    "the route with asterisk URI must explicitly handles OPTIONS"
                ).into());
            }
            if methods.iter().any(|method| method != Method::OPTIONS) {
                return Err(failure::format_err!(
                    "the route with asterisk URI must not accept any methods other than OPTIONS"
                ).into());
            }
        }

        let route_id = RouteId(endpoint.id, self.routes.len());
        for method in &methods {
            if endpoint.route_ids.contains_key(method) {
                return Err(Error::from(failure::format_err!(
                    "the route with the same URI and method is not supported."
                )));
            }
            endpoint.route_ids.insert(method.clone(), route_id);
        }

        self.routes.push(RouteData {
            id: route_id,
            uri: cx.uri,
            methods,
            handler: cx
                .handler
                .ok_or_else(|| failure::format_err!("default handler is not supported"))?,
        });

        Ok(())
    }

    pub(super) fn new_scope(&mut self, parent: ScopeId, scope: impl Scope) -> Result<()> {
        let pos = self.scopes.len();
        let id = ScopeId::Local(pos);

        let parents = match parent {
            ScopeId::Global => vec![ScopeId::Global],
            ScopeId::Local(i) => self.scopes[i]
                .parents
                .iter()
                .cloned()
                .chain(Some(parent))
                .collect(),
        };

        self.scopes.push(ScopeData {
            id,
            parents,
            prefix: None,
            modifiers: vec![],
        });

        scope
            .configure(&mut ScopeContext::new(self, id))
            .map_err(Into::into)?;

        Ok(())
    }

    pub(super) fn set_state<T>(&mut self, value: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        self.states.set(value, id);
    }

    pub(super) fn add_modifier(
        &mut self,
        modifier: impl Modifier + Send + Sync + 'static,
        id: ScopeId,
    ) {
        match id {
            ScopeId::Global => self.global_scope.modifiers.push(Box::new(modifier)),
            ScopeId::Local(i) => self.scopes[i].modifiers.push(Box::new(modifier)),
        }
    }

    pub(super) fn set_prefix(&mut self, id: ScopeId, prefix: Uri) {
        match id {
            ScopeId::Global => self.global_scope.prefix = Some(prefix),
            ScopeId::Local(id) => self.scopes[id].prefix = Some(prefix),
        }
    }
}
