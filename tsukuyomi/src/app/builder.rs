use {
    super::{
        fallback::Fallback,
        recognizer::Recognizer,
        tree::{Arena, NodeId},
        App, AppInner, Endpoint, Resource, ResourceId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never},
        handler::{Handler, ModifyHandler},
        output::Responder,
    },
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
            .finish()
    }
}

impl<S, M> Builder<S, M> {
    /// Merges the specified `Scope` into the global scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Builder<Chain<S, S2>, M> {
        Builder {
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
            prefix: self.prefix,
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
            fallback: self.fallback,
        }
    }

    /// Sets the instance of `Fallback` to the global scope.
    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Builder {
            fallback: Some(Box::new(fallback)),
            ..self
        }
    }

    /// Creates an `App` using the current configuration.
    pub fn build(self) -> super::Result<App>
    where
        S: Scope<M>,
    {
        let mut cx = ContextInner {
            resources: IndexMap::new(),
            scopes: Arena::new(ScopeData {
                prefix: self.prefix,
                fallback: self.fallback.map(Arc::new),
            }),
        };

        self.scope
            .configure(&mut Context {
                app: &mut cx,
                scope: NodeId::root(),
                modifier: self.modifier,
            })
            .map_err(Into::into)?;

        // create a route recognizer.
        let mut recognizer = Recognizer::default();
        for (uri, mut resource) in cx.resources {
            resource.update();
            recognizer.insert(uri.as_str(), resource)?;
        }

        Ok(App {
            inner: Arc::new(AppInner {
                recognizer,
                scopes: cx.scopes,
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

/// A trait representing a set of configurations within the scope.
pub trait Scope<M> {
    type Error: Into<super::Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Context<'_, M>) -> Result<(), Self::Error>;

    /// Consumes itself and returns a new `Scope` combined with the specified configuration.
    fn chain<S>(self, next: S) -> Chain<Self, S>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }
}

#[derive(Debug)]
struct ContextInner {
    resources: IndexMap<Uri, Resource>,
    scopes: Arena<ScopeData>,
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct Context<'a, M> {
    app: &'a mut ContextInner,
    scope: NodeId,
    modifier: M,
}

impl<'a, M> Context<'a, M> {
    pub(super) fn add_endpoint<H>(
        &mut self,
        uri: &Uri,
        mut methods: IndexSet<Method>,
        handler: H,
    ) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Output: Responder,
    {
        // build absolute URI.
        let uri = self.app.scopes[self.scope].data().prefix.join(&uri)?;

        let resource = {
            let id = ResourceId(self.app.resources.len());
            let scope = &self.app.scopes[self.scope];
            self.app
                .resources
                .entry(uri.clone())
                .or_insert_with(|| Resource {
                    id,
                    scope: scope.id(),
                    uri: uri.clone(),
                    endpoints: vec![],
                    allowed_methods: IndexMap::new(),
                    allowed_methods_value: HeaderValue::from_static(""),
                })
        };
        if self.scope != resource.scope {
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

    pub(super) fn add_scope<S, M2>(
        &mut self,
        prefix: &Uri,
        modifier: M2,
        fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
        new_scope: S,
    ) -> super::Result<()>
    where
        M: Clone,
        S: Scope<Chain<M, M2>>,
    {
        let scope = self.app.scopes.add_node(self.scope, {
            let parent = self.app.scopes[self.scope].data();
            ScopeData {
                prefix: parent.prefix.join(prefix)?,
                fallback: fallback.map(Arc::new).or_else(|| parent.fallback.clone()),
            }
        })?;

        new_scope
            .configure(&mut Context {
                app: &mut *self.app,
                scope,
                modifier: Chain::new(self.modifier.clone(), modifier),
            })
            .map_err(Into::into)?;

        Ok(())
    }
}

impl<M> Scope<M> for () {
    type Error = Never;

    fn configure(self, _: &mut Context<'_, M>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S1, S2, M> Scope<M> for Chain<S1, S2>
where
    S1: Scope<M>,
    S2: Scope<M>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M>) -> Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}
