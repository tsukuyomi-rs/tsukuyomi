use {
    super::{
        error::{Error, Result},
        fallback::Fallback,
        router::{Config, Endpoint, Recognizer, Resource, ResourceId, Router},
        scope::{Context as ScopeContext, Scope},
        App, AppInner, Uri,
    },
    crate::{handler::Handler, modifier::Modifier},
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

/// A builder object for constructing an instance of `App`.
pub struct Builder<S: Scope = ()> {
    scope: S,
    modifier: Option<Box<dyn Modifier + Send + Sync + 'static>>,
    fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
    prefix: Uri,
    config: Config,
}

impl<S> Default for Builder<S>
where
    S: Scope + Default,
{
    fn default() -> Self {
        Builder {
            scope: S::default(),
            modifier: None,
            fallback: None,
            prefix: Uri::root(),
            config: Config::default(),
        }
    }
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
    /// Merges the specified `Scope` into the global scope, *without* creating a new subscope.
    pub fn with(self, next_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: self.scope.chain(next_scope),
            modifier: self.modifier,
            fallback: self.fallback,
        }
    }

    /// Sets the prefix URI of the global scope.
    pub fn prefix(self, prefix: Uri) -> Self {
        Self { prefix, ..self }
    }

    pub fn modifier<M>(self, modifier: M) -> Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: self.scope,
            modifier: Some(Box::new(modifier)),
            fallback: self.fallback,
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: self.scope,
            modifier: self.modifier,
            fallback: Some(Box::new(fallback)),
        }
    }

    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(mut self, enabled: bool) -> Self {
        self.config.fallback_head = enabled;
        self
    }

    /// Creates an `App` using the current configuration.
    pub fn build(self) -> Result<App> {
        let mut cx = AppContext {
            resources: IndexMap::new(),
        };

        let global_scope = ScopeData {
            prefix: self.prefix,
            modifiers: self.modifier.into_iter().map(Arc::from).collect(),
            fallback: self.fallback.map(Arc::from),
        };
        self.scope
            .configure(&mut ScopeContext {
                app: &mut cx,
                data: &global_scope,
            }).map_err(Into::into)?;

        // create a route recognizer.
        let mut recognizer = Recognizer::default();
        for (uri, mut resource) in cx.resources {
            resource.update();
            recognizer.insert(uri.as_str(), resource)?;
        }

        Ok(App {
            inner: Arc::new(AppInner {
                router: Router {
                    recognizer,
                    config: self.config,
                },
            }),
        })
    }

    /// Creates a builder of HTTP server using the current configuration.
    pub fn build_server(self) -> Result<crate::server::Server<App>> {
        self.build().map(crate::server::Server::new)
    }
}

#[derive(Debug)]
pub(super) struct AppContext {
    resources: IndexMap<Uri, Resource>,
}

impl AppContext {
    pub(super) fn new_route<H>(
        &mut self,
        scope: &ScopeData,
        uri: Uri,
        mut methods: IndexSet<Method>,
        handler: H,
    ) -> Result<()>
    where
        H: Handler + Send + Sync + 'static,
    {
        // build absolute URI.
        let uri = { scope.prefix.join(&uri)? };

        let resource = {
            let id = ResourceId(self.resources.len());
            self.resources
                .entry(uri.clone())
                .or_insert_with(|| Resource {
                    id,
                    uri: uri.clone(),
                    endpoints: vec![],
                    modifiers: scope.modifiers.clone(),
                    fallback: scope.fallback.clone(),
                    allowed_methods: IndexMap::new(),
                    allowed_methods_value: HeaderValue::from_static(""),
                })
        };

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
            uri,
            methods,
            handler: Box::new(handler),
        });

        Ok(())
    }

    pub(super) fn new_scope(
        &mut self,
        parent: &ScopeData,
        prefix: Uri,
        modifier: Option<Box<dyn Modifier + Send + Sync + 'static>>,
        fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
        scope: impl Scope,
    ) -> Result<()> {
        let data = ScopeData {
            prefix: parent.prefix.join(&prefix)?,
            modifiers: {
                let mut modifiers = parent.modifiers.clone();
                if let Some(modifier) = modifier {
                    modifiers.push(Arc::from(modifier));
                }
                modifiers
            },
            fallback: match fallback {
                Some(fallback) => Some(Arc::from(fallback)),
                None => parent.fallback.clone(),
            },
        };

        scope
            .configure(&mut ScopeContext {
                app: self,
                data: &data,
            }).map_err(Into::into)?;

        Ok(())
    }
}

/// A type representing a set of data associated with the certain scope.
pub(super) struct ScopeData {
    pub(super) prefix: Uri,
    pub(super) modifiers: Vec<Arc<dyn Modifier + Send + Sync + 'static>>,
    pub(super) fallback: Option<Arc<dyn Fallback + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("prefix", &self.prefix)
            .finish()
    }
}
