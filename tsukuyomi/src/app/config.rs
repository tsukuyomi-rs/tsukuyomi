pub mod endpoint;
pub mod route;

pub mod prelude {
    #[doc(no_inline)]
    pub use super::route::route;
    #[doc(no_inline)]
    pub use super::{default_handler, empty, mount, Config, ConfigExt};

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

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct Scope<'a, M> {
    recognizer: &'a mut Recognizer<Resource>,
    scopes: &'a mut Arena<ScopeData>,
    modifier: &'a M,
    scope_id: NodeId,
}

impl<'a, M> Scope<'a, M> {
    /// Appends a `Handler` with the specified URI onto the current scope.
    pub fn at<H>(&mut self, uri: impl AsRef<str>, handler: H) -> super::Result<()>
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

    /// Creates a sub-scope with the provided prefix onto the current scope.
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

    /// Applies the specified configuration with a `ModifyHandler` on the current scope.
    pub fn modify<M2>(
        &mut self,
        modifier: M2,
        config: impl Config<Chain<&'a M, M2>>,
    ) -> super::Result<()> {
        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id: self.scope_id,
                modifier: &Chain::new(self.modifier, modifier),
            })
            .map_err(Into::into)
    }

    /// Registers a `Handler` at the current scope that will be called when the incoming
    /// request does not match any route.
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

pub fn empty() -> Empty {
    Empty(())
}

#[derive(Debug)]
pub struct Empty(());

impl<M> Config<M> for Empty {
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Creates a `Config` that registers a default handler onto the scope.
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

/// Creates a `Config` that creates a sub-scope with the provided prefix.
pub fn mount<P>(prefix: P) -> Mount<P, Empty>
where
    P: AsRef<str>,
{
    Mount {
        prefix,
        config: empty(),
    }
}

/// A `Config` that registers a sub-scope with a specific prefix.
#[derive(Debug)]
pub struct Mount<P, T> {
    prefix: P,
    config: T,
}

impl<P, T> Mount<P, T>
where
    P: AsRef<str>,
{
    pub fn with<T2>(self, config: T2) -> Mount<P, Chain<T, T2>> {
        Mount {
            prefix: self.prefix,
            config: Chain::new(self.config, config),
        }
    }
}

impl<P, T, M> Config<M> for Mount<P, T>
where
    P: AsRef<str>,
    T: Config<M>,
{
    type Error = super::Error;

    fn configure(self, scope: &mut Scope<'_, M>) -> Result<(), Self::Error> {
        scope.mount(self.prefix, self.config)
    }
}

/// Crates a `Config` that wraps a config with a `ModifyHandler`.
pub fn modify<M, T>(modifier: M, config: T) -> Modify<M, T> {
    Modify { modifier, config }
}

/// A `Config` that wraps a config with a `ModifyHandler`.
#[derive(Debug)]
pub struct Modify<M, T> {
    modifier: M,
    config: T,
}

impl<M, T, M2> Config<M2> for Modify<M, T>
where
    for<'a> T: Config<Chain<&'a M2, M>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Scope<'_, M2>) -> Result<(), Self::Error> {
        cx.modify(self.modifier, self.config)
    }
}

pub trait ConfigExt: Sized {
    fn with<T>(self, config: T) -> Chain<Self, T> {
        Chain::new(self, config)
    }

    /// Creates a `Config` with the specified `ModifyHandler`
    fn modify<M>(self, modifier: M) -> Modify<M, Self> {
        modify(modifier, self)
    }

    fn mount<P, T>(self, prefix: P, config: T) -> Chain<Self, Mount<P, Chain<Empty, T>>>
    where
        P: AsRef<str>,
    {
        self.with(mount(prefix).with(config))
    }

    fn default_handler<H>(self, default_handler: H) -> Chain<Self, DefaultHandler<H>> {
        self.with(DefaultHandler(default_handler))
    }
}

impl<T> ConfigExt for T {}
