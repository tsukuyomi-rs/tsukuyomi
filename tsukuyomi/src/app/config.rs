pub mod prelude {
    pub use super::super::route;
    pub use super::{default_handler, mount, with_modifier, AppConfig};
}

use {
    super::{
        recognizer::Recognizer,
        tree::{Arena, NodeId},
        App, AppInner, Resource, ResourceId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never, TryInto},
        handler::{Handler, ModifyHandler},
        output::Responder,
    },
    indexmap::IndexMap,
    std::sync::Arc,
};

/// Creates an `App` using the specified configuration.
pub fn configure(prefix: impl AsRef<str>, config: impl AppConfig<()>) -> super::Result<App> {
    let mut inner = AppConfigContextInner {
        resources: IndexMap::new(),
        scopes: Arena::new(ScopeData {
            prefix: prefix.as_ref().parse()?,
            fallback: None,
        }),
    };
    config
        .configure(&mut AppConfigContext {
            inner: &mut inner,
            scope_id: NodeId::root(),
            modifier: (),
        })
        .map_err(Into::into)?;

    // create a route recognizer.
    let mut recognizer = Recognizer::default();
    for (uri, resource) in inner.resources {
        recognizer.insert(uri.as_str(), resource)?;
    }

    Ok(App {
        inner: Arc::new(AppInner {
            recognizer,
            scopes: inner.scopes,
        }),
    })
}

/// A trait representing a set of elements that will be registered into a certain scope.
pub trait AppConfig<M> {
    type Error: Into<super::Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error>;
}

impl<M> AppConfig<M> for () {
    type Error = Never;

    fn configure(self, _: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S1, S2, M> AppConfig<M> for Chain<S1, S2>
where
    S1: AppConfig<M>,
    S2: AppConfig<M>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<M, S> AppConfig<M> for Option<S>
where
    S: AppConfig<M>,
{
    type Error = S::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<M, S, E> AppConfig<M> for Result<S, E>
where
    S: AppConfig<M>,
    E: Into<super::Error>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

#[derive(Debug)]
struct AppConfigContextInner {
    resources: IndexMap<Uri, Resource>,
    scopes: Arena<ScopeData>,
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct AppConfigContext<'a, M> {
    inner: &'a mut AppConfigContextInner,
    scope_id: NodeId,
    modifier: M,
}

impl<'a, M> AppConfigContext<'a, M> {
    #[doc(hidden)]
    pub fn add_route<H>(&mut self, uri: impl TryInto<Uri>, handler: H) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Send + Sync + 'static,
        M::Output: Responder,
    {
        let uri = uri.try_into()?;
        let uri = self.inner.scopes[self.scope_id].data.prefix.join(&uri)?;
        if self.inner.resources.contains_key(&uri) {
            return Err(super::Error::from(failure::format_err!(
                "detect the duplicated URI: {}",
                uri
            )));
        }

        let id = ResourceId(self.inner.resources.len());
        let scope = &self.inner.scopes[self.scope_id];
        self.inner.resources.insert(
            uri.clone(),
            Resource {
                id,
                scope: scope.id(),
                ancestors: scope
                    .ancestors()
                    .into_iter()
                    .cloned()
                    .chain(Some(scope.id()))
                    .collect(),
                uri: uri.clone(),
                handler: Box::new(self.modifier.modify(handler)),
            },
        );

        Ok(())
    }

    #[doc(hidden)]
    pub fn set_default_handler<H>(&mut self, default_handler: H) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Send + Sync + 'static,
        M::Output: Responder,
    {
        let handler = self.modifier.modify(default_handler);
        self.inner.scopes[self.scope_id].data.fallback = Some(Box::new(handler));
        Ok(())
    }

    #[doc(hidden)]
    pub fn add_scope<S>(&mut self, prefix: &str, scope: S) -> super::Result<()>
    where
        M: Clone,
        S: AppConfig<M>,
    {
        let prefix: Uri = prefix.parse()?;

        let scope_id = self.inner.scopes.add_node(self.scope_id, {
            let parent = &self.inner.scopes[self.scope_id].data;
            ScopeData {
                prefix: parent.prefix.join(&prefix)?,
                fallback: None,
            }
        })?;

        scope
            .configure(&mut AppConfigContext {
                inner: &mut *self.inner,
                scope_id,
                modifier: self.modifier.clone(),
            })
            .map_err(Into::into)?;

        Ok(())
    }

    #[doc(hidden)]
    pub fn with_modifier<M2>(&mut self, outer: M2) -> AppConfigContext<'_, Chain<M, M2>>
    where
        M: Clone,
    {
        AppConfigContext {
            inner: &mut *self.inner,
            scope_id: self.scope_id,
            modifier: Chain::new(self.modifier.clone(), outer),
        }
    }
}

/// Creates a `Scope` that registers the default handler onto the scope.
pub fn default_handler<H>(default_handler: H) -> DefaultHandler<H> {
    DefaultHandler(default_handler)
}

#[derive(Debug)]
pub struct DefaultHandler<H>(H);

impl<H, M> AppConfig<M> for DefaultHandler<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Handler: Send + Sync + 'static,
    M::Output: Responder,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        cx.set_default_handler(self.0)
    }
}

pub fn with_modifier<M, S>(modifier: M, scope: S) -> WithModifier<M, S>
where
    S: AppConfig<M>,
{
    WithModifier { modifier, scope }
}

/// A builder object for constructing an instance of `App`.
#[derive(Debug)]
pub struct WithModifier<M, S> {
    modifier: M,
    scope: S,
}

impl<M, S, M2> AppConfig<M2> for WithModifier<M, S>
where
    M2: Clone,
    S: AppConfig<Chain<M2, M>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M2>) -> std::result::Result<(), Self::Error> {
        self.scope
            .configure(&mut cx.with_modifier(self.modifier))
            .map_err(Into::into)
    }
}

/// A function that creates a `Mount` with the empty scope items.
pub fn mount<P, S>(prefix: P, scope: S) -> Mount<P, S>
where
    P: AsRef<str>,
    S: AppConfig<()>,
{
    Mount { prefix, scope }
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[derive(Debug)]
pub struct Mount<P, S> {
    prefix: P,
    scope: S,
}

impl<P, S> Mount<P, S>
where
    P: AsRef<str>,
{
    /// Sets the prefix of this scope.
    pub fn prefix<P2>(self, prefix: P2) -> Mount<P2, S> {
        Mount {
            prefix,
            scope: self.scope,
        }
    }

    /// Adds a `Scope` into this scope.
    pub fn with<S2>(self, scope: S2) -> Mount<P, Chain<S, S2>> {
        Mount {
            prefix: self.prefix,
            scope: Chain::new(self.scope, scope),
        }
    }
}

impl<P, S, M> AppConfig<M> for Mount<P, S>
where
    P: AsRef<str>,
    S: AppConfig<M>,
    M: Clone,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        cx.add_scope(self.prefix.as_ref(), self.scope)
    }
}
