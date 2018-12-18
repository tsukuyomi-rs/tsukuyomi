//! A collection of components for configuring `App`.

pub mod endpoint;
pub mod path;

pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{chain, path};

    #[doc(no_inline)]
    pub use super::{mount, Config, ConfigExt};

    pub mod endpoint {
        #[doc(no_inline)]
        pub use super::super::endpoint::{
            allow_only, any, connect, delete, get, head, options, patch, post, put, trace,
        };
    }
}

#[doc(no_inline)]
pub use crate::app::config::{Config, Error, Result, Scope};

use {
    crate::{
        app::config::Concurrency,
        handler::{Handler, ModifyHandler},
        util::Chain,
    },
    std::borrow::Cow,
};

/// Creates a `Config` that creates a sub-scope with the provided prefix.
pub fn mount<P>(prefix: P) -> Mount<P, ()>
where
    P: AsRef<str>,
{
    Mount { prefix, config: () }
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

impl<P, T, M, C> Config<M, C> for Mount<P, T>
where
    P: AsRef<str>,
    T: Config<M, C>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, scope: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
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

impl<M, T, M2, C> Config<M2, C> for Modify<M, T>
where
    for<'a> T: Config<Chain<&'a M2, M>, C>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M2, C>) -> std::result::Result<(), Self::Error> {
        cx.modify(self.modifier, self.config)
    }
}

pub trait ConfigExt: Sized {
    /// Creates a `Config` with the specified `ModifyHandler`
    fn modify<M>(self, modifier: M) -> Modify<M, Self> {
        modify(modifier, self)
    }
}

impl<T> ConfigExt for T {}

/// A `Config` that registers a route into a scope.
#[derive(Debug)]
pub struct Route<H> {
    path: Cow<'static, str>,
    handler: H,
}

impl<H> Route<H>
where
    H: Handler,
{
    /// Creates a `Route` with the speicified path and handler.
    pub fn new(path: impl Into<Cow<'static, str>>, handler: H) -> Self {
        Self {
            path: path.into(),
            handler,
        }
    }
}

impl<H, M, C> Config<M, C> for Route<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Handler: Into<C::Handler>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, scope: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        scope.route(self.path, self.handler)
    }
}
