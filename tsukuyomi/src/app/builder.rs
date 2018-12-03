use {
    super::{
        error::{Error, Result},
        fallback::Fallback,
        router::{Config, Endpoint, Recognizer, Resource, ResourceId, Router},
        scope::{Context as ScopeContext, Scope},
        App, AppInner, Uri,
    },
    crate::{common::Chain, handler::Handler},
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

/// A builder object for constructing an instance of `App`.
#[derive(Default)]
pub struct Builder<S = (), M = ()> {
    scope: S,
    modifier: M,
    fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
    prefix: Uri,
    config: Config,
}

#[cfg_attr(tarpaulin, skip)]
impl<S, M> fmt::Debug for Builder<S, M>
where
    S: fmt::Debug,
    M: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("scope", &self.scope)
            .field("modifier", &self.modifier)
            .field("prefix", &self.prefix)
            .field("config", &self.config)
            .finish()
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> Builder<S, M> {
    /// Merges the specified `Scope` into the global scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Builder<Chain<S, S2>, M> {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: Chain::new(self.scope, next_scope),
            modifier: self.modifier,
            fallback: self.fallback,
        }
    }

    /// Sets the prefix URI of the global scope.
    pub fn prefix(self, prefix: Uri) -> Self {
        Self { prefix, ..self }
    }

    pub fn modifier<M2>(self, modifier: M2) -> Builder<S, Chain<M, M2>> {
        Builder {
            config: self.config,
            prefix: self.prefix,
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
            fallback: self.fallback,
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Builder {
            fallback: Some(Box::new(fallback)),
            ..self
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
    pub fn build(self) -> Result<App>
    where
        S: Scope<M>,
    {
        let mut cx = AppContext {
            resources: IndexMap::new(),
        };

        let global_scope = ScopeData {
            prefix: self.prefix,
            fallback: self.fallback.map(Arc::from),
        };
        self.scope
            .configure(&mut ScopeContext {
                app: &mut cx,
                data: &global_scope,
                modifier: self.modifier,
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
    pub fn build_server(self) -> Result<crate::server::Server<App>>
    where
        S: Scope<M>,
    {
        self.build().map(crate::server::Server::new)
    }
}

#[derive(Debug)]
pub(super) struct AppContext {
    resources: IndexMap<Uri, Resource>,
}

impl AppContext {
    pub(super) fn new_route(
        &mut self,
        scope: &ScopeData,
        uri: Uri,
        mut methods: IndexSet<Method>,
        handler: impl Handler<
                Handle = impl crate::handler::AsyncResult<crate::output::Output> + Send + 'static,
            > + Send
            + Sync
            + 'static,
    ) -> Result<()> {
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
            handler: handler.into(),
        });

        Ok(())
    }

    pub(super) fn new_scope<M>(
        &mut self,
        parent: &ScopeData,
        prefix: Uri,
        modifier: M,
        fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
        scope: impl Scope<M>,
    ) -> Result<()> {
        let data = ScopeData {
            prefix: parent.prefix.join(&prefix)?,
            fallback: match fallback {
                Some(fallback) => Some(Arc::from(fallback)),
                None => parent.fallback.clone(),
            },
        };

        scope
            .configure(&mut ScopeContext {
                app: self,
                data: &data,
                modifier,
            }).map_err(Into::into)?;

        Ok(())
    }
}

/// A type representing a set of data associated with the certain scope.
pub(super) struct ScopeData {
    pub(super) prefix: Uri,
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
