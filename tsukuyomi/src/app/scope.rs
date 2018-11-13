use crate::internal::scoped_map::ScopeId;

use super::handler::Modifier;
use super::route::RouteConfig;
use super::{AppBuilderContext, AppError, AppResult};

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ScopeConfig {
    type Error: Into<AppError>;

    fn configure(self, cx: &mut ScopeContext<'_>) -> Result<(), Self::Error>;
}

impl ScopeConfig for () {
    type Error = crate::error::Never;

    fn configure(self, _: &mut ScopeContext<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(super) fn scope_config<F, E>(f: F) -> impl ScopeConfig<Error = E>
where
    F: FnOnce(&mut ScopeContext<'_>) -> Result<(), E>,
    E: Into<AppError>,
{
    #[allow(missing_debug_implementations)]
    struct ScopeConfigFn<F>(F);

    impl<F, E> ScopeConfig for ScopeConfigFn<F>
    where
        F: FnOnce(&mut ScopeContext<'_>) -> Result<(), E>,
        E: Into<AppError>,
    {
        type Error = E;

        fn configure(self, cx: &mut ScopeContext<'_>) -> Result<(), Self::Error> {
            (self.0)(cx)
        }
    }

    ScopeConfigFn(f)
}

pub fn builder() -> ScopeBuilder<()> {
    ScopeBuilder {
        config: (),
        prefix: None,
    }
}

pub fn with_prefix(prefix: &str) -> ScopeBuilder<()> {
    ScopeBuilder {
        config: (),
        prefix: Some(prefix.to_owned()),
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct ScopeBuilder<S: ScopeConfig = ()> {
    prefix: Option<String>,
    config: S,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> ScopeBuilder<S>
where
    S: ScopeConfig,
{
    /// Adds a route into the current scope.
    pub fn route(
        self,
        route: impl RouteConfig,
    ) -> ScopeBuilder<impl ScopeConfig<Error = AppError>> {
        let Self { config, prefix } = self;
        ScopeBuilder {
            prefix,
            config: scope_config(move |scope| {
                config.configure(scope).map_err(Into::into)?;
                scope.route(route)?;
                Ok(())
            }),
        }
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn mount(
        self,
        scope: impl ScopeConfig,
    ) -> ScopeBuilder<impl ScopeConfig<Error = AppError>> {
        let Self { config, prefix } = self;
        ScopeBuilder {
            prefix,
            config: scope_config(move |cx| {
                config.configure(cx).map_err(Into::into)?;
                cx.mount(scope)?;
                Ok(())
            }),
        }
    }

    /// Adds a *scope-local* variable into the application.
    pub fn state<T>(self, state: T) -> ScopeBuilder<impl ScopeConfig<Error = S::Error>>
    where
        T: Send + Sync + 'static,
    {
        let Self { config, prefix } = self;
        ScopeBuilder {
            prefix,
            config: scope_config(move |cx| {
                config.configure(cx)?;
                cx.state(state);
                Ok(())
            }),
        }
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier<M>(self, modifier: M) -> ScopeBuilder<impl ScopeConfig<Error = S::Error>>
    where
        M: Modifier + Send + Sync + 'static,
    {
        let Self { config, prefix } = self;
        ScopeBuilder {
            prefix,
            config: scope_config(move |cx| {
                config.configure(cx)?;
                cx.modifier(modifier);
                Ok(())
            }),
        }
    }

    pub fn with(self, other: impl ScopeConfig) -> ScopeBuilder<impl ScopeConfig<Error = AppError>> {
        let Self { config, prefix } = self;
        ScopeBuilder {
            prefix,
            config: scope_config(move |cx| {
                config.configure(cx).map_err(Into::into)?;
                other.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }
}

impl<S> ScopeConfig for ScopeBuilder<S>
where
    S: ScopeConfig,
{
    type Error = AppError;

    fn configure(self, cx: &mut ScopeContext<'_>) -> Result<(), Self::Error> {
        self.config.configure(cx).map_err(Into::into)?;
        if let Some(prefix) = self.prefix {
            cx.prefix(&prefix)?;
        }
        Ok(())
    }
}

/// A proxy object for configuration of a scope.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct ScopeContext<'a> {
    cx: &'a mut AppBuilderContext,
    id: ScopeId,
}

impl<'a> ScopeContext<'a> {
    pub(super) fn new(cx: &'a mut AppBuilderContext, id: ScopeId) -> Self {
        Self { cx, id }
    }

    /// Adds a route into the current scope.
    pub fn route<R>(&mut self, route: R) -> AppResult<&mut Self>
    where
        R: RouteConfig,
    {
        self.cx.new_route(self.id, route)?;
        Ok(self)
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn mount<S>(&mut self, scope: S) -> AppResult<&mut Self>
    where
        S: ScopeConfig,
    {
        self.cx.new_scope(self.id, scope)?;
        Ok(self)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn state<T>(&mut self, value: T) -> &mut Self
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id);
        self
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier<M>(&mut self, modifier: M) -> &mut Self
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.cx.add_modifier(self.id, modifier);
        self
    }

    pub fn prefix(&mut self, prefix: &str) -> AppResult<&mut Self> {
        self.cx.set_prefix(self.id, prefix)?;
        Ok(self)
    }
}
