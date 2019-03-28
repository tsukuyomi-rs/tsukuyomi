//! A collection of components for configuring `App`.

#![deprecated(since = "0.6.0", note = "The old API will be removed at the next version.")]

pub mod endpoint {
    #[doc(inline)]
    pub use crate::endpoint::builder::*;
}

pub mod path {
    #[doc(inline)]
    pub use crate::app::path::{Path, PathExtractor};
}

pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{chain, path};

    #[doc(no_inline)]
    pub use super::Config;
    #[allow(deprecated)]
    #[doc(no_inline)]
    pub use super::{mount, ConfigExt};

    pub mod endpoint {
        #[doc(no_inline)]
        pub use super::super::endpoint::{
            allow_only, any, call, call_async, connect, delete, get, head, options, patch, post,
            put, reply, trace,
        };
    }
}

#[doc(no_inline)]
pub use crate::app::config::{Config, Error, IsConfig, Result, Scope};

use {
    crate::{
        app::concurrency::Concurrency,
        handler::{Handler, ModifyHandler},
        util::Chain,
    },
    std::fmt,
};

/// Creates a `Config` that creates a sub-scope with the provided prefix.
#[allow(deprecated)]
pub fn mount<P>(prefix: P) -> Mount<P, ()>
where
    P: AsRef<str>,
{
    Mount { prefix, config: () }
}

/// A `Config` that registers a sub-scope with a specific prefix.
pub struct Mount<P, T> {
    prefix: P,
    config: T,
}

#[allow(deprecated)]
impl<P, T> fmt::Debug for Mount<P, T>
where
    P: fmt::Debug,
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mount")
            .field("prefix", &self.prefix)
            .field("config", &self.config)
            .finish()
    }
}

#[allow(deprecated)]
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

#[allow(deprecated)]
impl<P, T> IsConfig for Mount<P, T>
where
    P: AsRef<str>,
    T: IsConfig,
{
}

#[allow(deprecated)]
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
#[allow(deprecated)]
pub fn modify<M, T>(modifier: M, config: T) -> Modify<M, T> {
    Modify { modifier, config }
}

/// A `Config` that wraps a config with a `ModifyHandler`.
pub struct Modify<M, T> {
    modifier: M,
    config: T,
}

#[allow(deprecated)]
impl<M, T> fmt::Debug for Modify<M, T>
where
    M: fmt::Debug,
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Modify")
            .field("modifier", &self.modifier)
            .field("config", &self.config)
            .finish()
    }
}

#[allow(deprecated)]
impl<M, T> IsConfig for Modify<M, T> where T: IsConfig {}

#[allow(deprecated)]
impl<M, T, M2, C> Config<M2, C> for Modify<M, T>
where
    T: for<'a> Config<Chain<&'a M2, M>, C>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M2, C>) -> std::result::Result<(), Self::Error> {
        cx.modify(self.modifier, self.config)
    }
}

/// A set of extension methods for constructing the complex `Config`.
#[allow(deprecated)]
pub trait ConfigExt: IsConfig + Sized {
    /// Creates a `Config` that applies `Self` and the specified configuration in order.
    fn chain<T>(self, next: T) -> Chain<Self, T>
    where
        T: IsConfig,
    {
        Chain::new(self, next)
    }

    /// Creates a `Config` that applies the specified `ModifyHandler` to all `Handler`s
    /// registered by `Self`.
    fn modify<M>(self, modifier: M) -> Modify<M, Self> {
        modify(modifier, self)
    }
}

#[allow(deprecated)]
impl<T: IsConfig> ConfigExt for T {}

/// A `Config` that registers a route into a scope.
pub struct Route<H> {
    pub(crate) handler: H,
}

#[allow(deprecated)]
impl<H> fmt::Debug for Route<H>
where
    H: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("handler", &self.handler)
            .finish()
    }
}

#[allow(deprecated)]
impl<H> Route<H>
where
    H: Handler,
{
    /// Creates a `Route` with the speicified handler.
    pub fn new(handler: H) -> Self {
        Self { handler }
    }
}

#[allow(deprecated)]
impl<H> IsConfig for Route<H> where H: Handler {}

#[allow(deprecated)]
impl<H, M, C> Config<M, C> for Route<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Handler: Into<C::Handler>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, scope: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        scope.route(self.handler)
    }
}
