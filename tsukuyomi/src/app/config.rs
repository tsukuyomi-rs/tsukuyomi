pub mod endpoint;
pub mod route;

pub mod prelude {
    #[doc(no_inline)]
    pub use super::route::route;
    #[doc(no_inline)]
    pub use super::{default_handler, mount, with_modifier, Config};

    pub mod endpoint {
        #[doc(no_inline)]
        pub use super::super::endpoint::{
            allow_only, any, connect, delete, get, head, options, patch, post, put, trace,
        };
    }
}

use {
    super::{
        recognizer::Recognizer,
        tree::{Arena, NodeId},
        App, AppInner, Resource, ResourceId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never},
        handler::{Handler, ModifyHandler},
        output::Responder,
    },
    std::sync::Arc,
};

/// Creates an `App` using the specified configuration.
pub fn configure(prefix: impl AsRef<str>, config: impl Config<()>) -> super::Result<App> {
    let mut recognizer = Recognizer::default();
    let mut scopes = Arena::new(ScopeData {
        prefix: prefix.as_ref().parse()?,
        fallback: None,
    });
    config
        .configure(&mut Scope {
            recognizer: &mut recognizer,
            scopes: &mut scopes,
            scope_id: NodeId::root(),
            modifier: &(),
        })
        .map_err(Into::into)?;

    Ok(App {
        inner: Arc::new(AppInner { recognizer, scopes }),
    })
}

/// A trait representing a set of elements that will be registered into a certain scope.
pub trait Config<M> {
    type Error: Into<super::Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error>;
}

impl<F, M, E> Config<M> for F
where
    F: FnOnce(&mut Scope<'_, M>) -> Result<(), E>,
    E: Into<super::Error>,
{
    type Error = E;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        self(cx)
    }
}

impl<M> Config<M> for () {
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S1, S2, M> Config<M> for Chain<S1, S2>
where
    S1: Config<M>,
    S2: Config<M>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<M, S> Config<M> for Option<S>
where
    S: Config<M>,
{
    type Error = S::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<M, S, E> Config<M> for Result<S, E>
where
    S: Config<M>,
    E: Into<super::Error>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct Scope<'a, M> {
    recognizer: &'a mut Recognizer<Resource>,
    scopes: &'a mut Arena<ScopeData>,
    scope_id: NodeId,
    modifier: &'a M,
}

impl<'a, M> Scope<'a, M> {
    pub fn route<H>(&mut self, uri: impl AsRef<str>, handler: H) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Send + Sync + 'static,
        M::Output: Responder,
    {
        let uri: Uri = uri.as_ref().parse()?;
        let uri = self.scopes[self.scope_id].data.prefix.join(&uri)?;

        let id = ResourceId(self.recognizer.len());
        let scope = &self.scopes[self.scope_id];
        self.recognizer.insert(
            uri.as_str(),
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
        )?;

        Ok(())
    }

    pub fn default_handler<H>(&mut self, default_handler: H) -> super::Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Send + Sync + 'static,
        M::Output: Responder,
    {
        let handler = self.modifier.modify(default_handler);
        self.scopes[self.scope_id].data.fallback = Some(Box::new(handler));
        Ok(())
    }

    pub fn mount(&mut self, prefix: impl AsRef<str>, config: impl Config<M>) -> super::Result<()> {
        let prefix: Uri = prefix.as_ref().parse()?;

        let scope_id = self.scopes.add_node(self.scope_id, {
            let parent = &self.scopes[self.scope_id].data;
            ScopeData {
                prefix: parent.prefix.join(&prefix)?,
                fallback: None,
            }
        })?;

        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id,
                modifier: &*self.modifier,
            })
            .map_err(Into::into)?;

        Ok(())
    }

    pub fn modify<M2>(
        &mut self,
        outer: M2,
        config: impl for<'m> Config<Chain<&'m M, M2>>,
    ) -> super::Result<()> {
        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id: self.scope_id,
                modifier: &Chain::new(&*self.modifier, outer),
            })
            .map_err(Into::into)
    }
}

/// Creates a `Scope` that registers the default handler onto the scope.
pub fn default_handler<H>(default_handler: H) -> DefaultHandler<H> {
    DefaultHandler(default_handler)
}

#[derive(Debug)]
pub struct DefaultHandler<H>(H);

impl<H, M> Config<M> for DefaultHandler<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Handler: Send + Sync + 'static,
    M::Output: Responder,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        cx.default_handler(self.0)
    }
}

pub fn with_modifier<M, S>(modifier: M, scope: S) -> WithModifier<M, S>
where
    S: Config<M>,
{
    WithModifier { modifier, scope }
}

/// A builder object for constructing an instance of `App`.
#[derive(Debug)]
pub struct WithModifier<M, S> {
    modifier: M,
    scope: S,
}

impl<M, S, M2> Config<M2> for WithModifier<M, S>
where
    for<'a> S: Config<Chain<&'a M2, M>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M2>) -> std::result::Result<(), Self::Error> {
        cx.modify(self.modifier, self.scope)
    }
}

/// A function that creates a `Mount` with the empty scope items.
pub fn mount<P, S>(prefix: P, scope: S) -> Mount<P, S>
where
    P: AsRef<str>,
    S: Config<()>,
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

impl<P, S, M> Config<M> for Mount<P, S>
where
    P: AsRef<str>,
    S: Config<M>,
    M: Clone,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        cx.mount(self.prefix, self.scope)
    }
}
