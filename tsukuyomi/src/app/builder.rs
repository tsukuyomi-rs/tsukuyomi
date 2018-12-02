use {
    super::{
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
        router::{Config, Endpoint, Recognizer, Resource, ResourceId, Router, Scope as ScopeData},
        scope::{Context as ScopeContext, Scope},
        scoped_map::{Builder as ScopedContainerBuilder, ScopeId},
        App, AppInner, Uri,
    },
    crate::modifier::Modifier,
    http::{header::HeaderValue, Method},
    indexmap::IndexMap,
    std::{fmt, sync::Arc},
};

#[allow(deprecated)]
use super::route::{Context as RouteContext, Route};

/// A builder object for constructing an instance of `App`.
#[derive(Default)]
pub struct Builder<S: Scope = ()> {
    scope: S,
    prefix: Option<Uri>,
    config: Config,
}

#[cfg_attr(tarpaulin, skip)]
impl<S> fmt::Debug for Builder<S>
where
    S: Scope + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("scope", &self.scope)
            .field("prefix", &self.prefix)
            .field("config", &self.config)
            .finish()
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    #[doc(hidden)]
    #[deprecated(since = "0.4.2", note = "use `Builder::with` instead.")]
    #[allow(deprecated)]
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>> {
        self.with(super::scope::raw(move |cx| cx.add_route(route)))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.4.2", note = "use `Builder::with` instead")]
    pub fn mount(self, new_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        self.with(super::scope::raw(move |cx| cx.add_scope(new_scope)))
    }

    /// Merges the specified `Scope` into the global scope, *without* creating a new subscope.
    pub fn with(self, next_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: self.scope.chain(next_scope),
        }
    }

    #[doc(hidden)]
    #[deprecated(since = "0.4.2", note = "use `Builder::with` instead")]
    #[allow(deprecated)]
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = Error>>
    where
        T: Send + Sync + 'static,
    {
        self.with(super::directives::state(state))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.4.2", note = "use `Builder::with` instead")]
    #[allow(deprecated)]
    pub fn modifier<M>(self, modifier: M) -> Builder<impl Scope<Error = Error>>
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.with(super::directives::modifier(modifier))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.4.2", note = "use `Builder::with` instead")]
    #[allow(deprecated)]
    pub fn fallback<F>(self, fallback: F) -> Builder<impl Scope<Error = Error>>
    where
        F: Fallback + Send + Sync + 'static,
    {
        self.state(FallbackInstance::from(fallback))
    }

    /// Sets the prefix URI of the global scope.
    pub fn prefix(self, prefix: Uri) -> Builder<S> {
        Self {
            prefix: Some(prefix),
            ..self
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
        build(self.scope, self.prefix, self.config)
    }

    /// Creates a builder of HTTP server using the current configuration.
    pub fn build_server(self) -> Result<crate::server::Server<App>> {
        self.build().map(crate::server::Server::new)
    }
}

fn build(scope: impl Scope, prefix: Option<Uri>, config: Config) -> Result<App> {
    let mut cx = AppContext {
        resources: IndexMap::new(),
        scopes: vec![],
        global_scope: ScopeData {
            id: ScopeId::Global,
            parents: vec![],
            prefix: prefix.clone(),
            uri: prefix,
            modifiers: vec![],
        },
        states: ScopedContainerBuilder::default(),
    };

    scope
        .configure(&mut ScopeContext::new(&mut cx, ScopeId::Global))
        .map_err(Into::into)?;

    let AppContext {
        resources,
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
    for (uri, mut resource) in resources {
        resource.update();
        recognizer.insert(uri.as_str(), resource)?;
    }

    Ok(App {
        inner: Arc::new(AppInner {
            router: Router {
                recognizer,
                config,
                scopes,
                global_scope,
            },
            data: states,
        }),
    })
}

#[derive(Debug)]
pub(super) struct AppContext {
    resources: IndexMap<Uri, Resource>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,
    states: ScopedContainerBuilder,
}

impl AppContext {
    #[allow(deprecated)]
    pub(super) fn new_route(&mut self, scope_id: ScopeId, route: impl Route) -> Result<()> {
        let mut cx = RouteContext {
            uri: Uri::root(),
            methods: None,
            handler: None,
        };
        route.configure(&mut cx).map_err(Into::into)?;

        // build absolute URI.
        let uri = {
            let scope = match scope_id {
                ScopeId::Global => &self.global_scope,
                ScopeId::Local(i) => &self.scopes[i],
            };
            super::uri::join_all(scope.uri.iter().chain(Some(&cx.uri)))?
        };

        // collect a chain of scope IDs where this endpoint belongs.
        let parents = match scope_id {
            ScopeId::Local(i) => self.scopes[i]
                .parents
                .iter()
                .cloned()
                .chain(Some(scope_id))
                .collect(),
            ScopeId::Global => vec![ScopeId::Global],
        };

        let resource = {
            let pos = self.resources.len();
            self.resources
                .entry(uri.clone())
                .or_insert_with(|| Resource {
                    id: ResourceId(scope_id, pos),
                    uri: uri.clone(),
                    endpoints: vec![],
                    allowed_methods: IndexMap::new(),
                    allowed_methods_value: HeaderValue::from_static(""),
                    parents,
                })
        };

        if scope_id != resource.id.0 {
            return Err(Error::from(failure::format_err!(
                "all endpoints with the same URI belong to the same scope"
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

        let endpoint_id = resource.allowed_methods.len();
        for method in &methods {
            if resource.allowed_methods.contains_key(method) {
                return Err(Error::from(failure::format_err!(
                    "the route with the same URI and method is not supported."
                )));
            }
            resource.allowed_methods.insert(method.clone(), endpoint_id);
        }

        resource.endpoints.push(Endpoint {
            id: endpoint_id,
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

        let (parents, uri) = match parent {
            ScopeId::Global => (vec![ScopeId::Global], self.global_scope.uri.clone()),
            ScopeId::Local(i) => {
                let mut parents = self.scopes[i].parents.clone();
                parents.push(parent);
                (parents, self.scopes[i].uri.clone())
            }
        };

        self.scopes.push(ScopeData {
            id,
            parents,
            prefix: None,
            uri,
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

    pub(super) fn set_prefix(&mut self, id: ScopeId, prefix: Uri) -> Result<()> {
        match id {
            ScopeId::Global => {
                self.global_scope.prefix = Some(prefix);
                self.global_scope.uri = self.global_scope.prefix.clone();
            }
            ScopeId::Local(id) => {
                self.scopes[id].prefix = Some(prefix);

                self.scopes[id].uri = {
                    let parent_scope = match self.scopes[id].parents.last().cloned() {
                        Some(ScopeId::Global) => &self.global_scope,
                        Some(ScopeId::Local(i)) => &self.scopes[i],
                        None => unreachable!("the local scope must have at least one parent."),
                    };
                    match (&parent_scope.uri, &self.scopes[id].prefix) {
                        (Some(uri), Some(prefix)) => uri.join(prefix).map(Some)?,
                        (Some(uri), None) => Some(uri.clone()),
                        (None, uri) => uri.clone(),
                    }
                };
            }
        }

        Ok(())
    }
}
