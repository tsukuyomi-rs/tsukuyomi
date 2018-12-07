use {
    super::{
        fallback::Fallback,
        recognizer::Recognizer,
        route::Methods,
        tree::{Arena, NodeId},
        App, AppInner, Endpoint, Resource, ResourceId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never, TryInto},
        handler::{Handler, ModifyHandler},
        output::Responder,
    },
    http::{header::HeaderValue, Method},
    indexmap::IndexMap,
    std::{fmt, sync::Arc},
};

/// A builder object for constructing an instance of `App`.
#[derive(Debug, Default)]
pub struct Builder<S = (), M = ()> {
    global_scope: Mount<S, M>,
}

impl<S, M> Builder<S, M> {
    /// Merges the specified `Scope` into the global scope.
    pub fn with<S2>(self, scope: S2) -> Builder<Chain<S, S2>, M> {
        Builder {
            global_scope: self.global_scope.with(scope),
        }
    }

    /// Applies the specified `modifier` to the global scope.
    pub fn modify<M2>(self, modifier: M2) -> Builder<S, Chain<M, M2>> {
        Builder {
            global_scope: self.global_scope.modify(modifier),
        }
    }

    /// Sets the instance of `Fallback` to the global scope.
    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Self {
            global_scope: self.global_scope.fallback(fallback),
        }
    }

    /// Sets the prefix URI of the global scope.
    pub fn prefix(self, prefix: impl AsRef<str>) -> super::Result<Self> {
        Ok(Self {
            global_scope: self.global_scope.prefix(prefix)?,
        })
    }

    /// Creates an `App` using the current configuration.
    pub fn build(self) -> super::Result<App>
    where
        S: Scope<M>,
    {
        let mut inner = ScopeContextInner {
            resources: IndexMap::new(),
            scopes: Arena::new(ScopeData {
                prefix: self.global_scope.prefix,
                fallback: self.global_scope.fallback.map(Arc::from),
            }),
        };
        self.global_scope
            .scope
            .configure(&mut ScopeContext {
                inner: &mut inner,
                scope_id: NodeId::root(),
                modifier: self.global_scope.modifier,
            })
            .map_err(Into::into)?;

        // create a route recognizer.
        let mut recognizer = Recognizer::default();
        for (uri, mut resource) in inner.resources {
            resource.update();
            recognizer.insert(uri.as_str(), resource)?;
        }

        Ok(App {
            inner: Arc::new(AppInner {
                recognizer,
                scopes: inner.scopes,
            }),
        })
    }

    /// Creates a builder of HTTP server using the current configuration.
    pub fn build_server(self) -> super::Result<crate::server::Server<App>>
    where
        S: Scope<M>,
    {
        self.build().map(crate::server::Server::new)
    }
}

/// A function that creates a `Mount` with the empty scope items.
pub fn mount(prefix: impl AsRef<str>) -> super::Result<Mount<(), ()>> {
    Ok(Mount {
        scope: (),
        modifier: (),
        fallback: None,
        prefix: prefix.as_ref().parse()?,
    })
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[derive(Default)]
pub struct Mount<S = (), M = ()> {
    scope: S,
    modifier: M,
    fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
    prefix: Uri,
}

#[cfg_attr(tarpaulin, skip)]
impl<S, M> fmt::Debug for Mount<S, M>
where
    S: fmt::Debug,
    M: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mount")
            .field("scope", &self.scope)
            .field("modifier", &self.modifier)
            .field("fallback", &self.fallback.as_ref().map(|_| "<fallback>"))
            .field("prefix", &self.prefix)
            .finish()
    }
}

impl<S, M> Mount<S, M> {
    /// Merges the specified `Scope` into the inner scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Mount<Chain<S, S2>, M> {
        Mount {
            scope: Chain::new(self.scope, next_scope),
            modifier: self.modifier,
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn modify<M2>(self, modifier: M2) -> Mount<S, Chain<M, M2>> {
        Mount {
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Self {
            fallback: Some(Box::new(fallback)),
            ..self
        }
    }

    pub fn prefix(self, prefix: impl AsRef<str>) -> super::Result<Self> {
        Ok(Self {
            prefix: prefix.as_ref().parse()?,
            ..self
        })
    }
}

impl<S, M1, M2> Scope<M1> for Mount<S, M2>
where
    M1: Clone,
    S: Scope<Chain<M1, M2>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut ScopeContext<'_, M1>) -> std::result::Result<(), Self::Error> {
        cx.add_scope(self)
    }
}

/// A trait representing a set of elements that will be registered into a certain scope.
pub trait Scope<M> {
    type Error: Into<super::Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut ScopeContext<'_, M>) -> Result<(), Self::Error>;

    /// Creates a new `Scope` combined with itself and the specified instance of `Scope`.
    fn chain<S>(self, next: S) -> Chain<Self, S>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }
}

impl<M> Scope<M> for () {
    type Error = Never;

    fn configure(self, _: &mut ScopeContext<'_, M>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S1, S2, M> Scope<M> for Chain<S1, S2>
where
    S1: Scope<M>,
    S2: Scope<M>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut ScopeContext<'_, M>) -> Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ScopeContextInner {
    resources: IndexMap<Uri, Resource>,
    scopes: Arena<ScopeData>,
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct ScopeContext<'a, M> {
    inner: &'a mut ScopeContextInner,
    scope_id: NodeId,
    modifier: M,
}

impl<'a, M> ScopeContext<'a, M> {
    #[doc(hidden)]
    pub fn add_route<H>(
        &mut self,
        uri: impl TryInto<Uri>,
        methods: impl TryInto<Methods>,
        handler: H,
    ) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Send + Sync + 'static,
        M::Output: Responder,
    {
        let uri = uri.try_into()?;
        let mut methods = methods.try_into()?.0;

        let uri = self.inner.scopes[self.scope_id].data().prefix.join(&uri)?;

        let resource = {
            let id = ResourceId(self.inner.resources.len());
            let scope = &self.inner.scopes[self.scope_id];
            self.inner
                .resources
                .entry(uri.clone())
                .or_insert_with(|| Resource {
                    id,
                    scope: scope.id(),
                    ancestors: scope.ancestors().chain(Some(scope.id())).collect(),
                    uri: uri.clone(),
                    endpoints: vec![],
                    allowed_methods: IndexMap::new(),
                    allowed_methods_value: HeaderValue::from_static(""),
                })
        };

        if self.scope_id != resource.scope {
            return Err(failure::format_err!("different scope id").into());
        }

        if methods.is_empty() {
            methods.insert(Method::GET);
        }

        if uri.is_asterisk() {
            if !methods.contains(&Method::OPTIONS) {
                return Err(failure::format_err!(
                    "the route with asterisk URI must explicitly handles OPTIONS"
                )
                .into());
            }
            if methods.iter().any(|method| method != Method::OPTIONS) {
                return Err(failure::format_err!(
                    "the route with asterisk URI must not accept any methods other than OPTIONS"
                )
                .into());
            }
        }

        let endpoint_id = resource.allowed_methods.len();
        for method in &methods {
            if resource.allowed_methods.contains_key(method) {
                return Err(super::Error::from(failure::format_err!(
                    "the route with the same URI and method is not supported."
                )));
            }
            resource.allowed_methods.insert(method.clone(), endpoint_id);
        }

        resource.endpoints.push(Endpoint {
            id: endpoint_id,
            uri,
            methods,
            handler: self.modifier.modify(handler).into(),
        });

        Ok(())
    }

    pub(super) fn add_scope<S, M2>(&mut self, mount: Mount<S, M2>) -> super::Result<()>
    where
        M: Clone,
        S: Scope<Chain<M, M2>>,
    {
        let scope_id = self.inner.scopes.add_node(self.scope_id, {
            let parent = self.inner.scopes[self.scope_id].data();
            ScopeData {
                prefix: parent.prefix.join(mount.prefix)?,
                fallback: mount
                    .fallback
                    .map(Arc::from)
                    .or_else(|| parent.fallback.clone()),
            }
        })?;

        mount
            .scope
            .configure(&mut ScopeContext {
                inner: &mut *self.inner,
                scope_id,
                modifier: Chain::new(self.modifier.clone(), mount.modifier),
            })
            .map_err(Into::into)?;

        Ok(())
    }
}
