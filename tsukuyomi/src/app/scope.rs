use crate::scoped_map::ScopeId;
use crate::uri::Uri;

use super::builder::AppContext;
use super::error::{Error, Result};
use super::handler::Modifier;
use super::route::Route;

pub trait Scope {
    type Error: Into<Error>;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error>;
}

impl Scope for () {
    type Error = crate::error::Never;

    fn configure(self, _: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

pub(super) fn raw<F, E>(f: F) -> impl Scope<Error = E>
where
    F: FnOnce(&mut Context<'_>) -> std::result::Result<(), E>,
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, E> Scope for Raw<F>
    where
        F: FnOnce(&mut Context<'_>) -> std::result::Result<(), E>,
        E: Into<Error>,
    {
        type Error = E;

        fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
            (self.0)(cx)
        }
    }

    Raw(f)
}

#[derive(Debug, Default)]
pub struct Builder<S: Scope = ()> {
    scope: S,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    /// Adds a route into the current scope.
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_route(route)?;
                Ok(())
            }),
        }
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn mount(self, scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_scope(scope)?;
                Ok(())
            }),
        }
    }

    /// Merges the specified `Scope` into the current scope, *without* creating a new scope.
    pub fn with(self, scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }

    /// Adds a *scope-local* variable into the application.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>>
    where
        T: Send + Sync + 'static,
    {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.set_state(state);
                Ok(())
            }),
        }
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier<M>(self, modifier: M) -> Builder<impl Scope<Error = S::Error>>
    where
        M: Modifier + Send + Sync + 'static,
    {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.add_modifier(modifier);
                Ok(())
            }),
        }
    }

    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = S::Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.set_prefix(prefix);
                Ok(())
            }),
        }
    }
}

impl<S> Scope for Builder<S>
where
    S: Scope,
{
    type Error = S::Error;

    #[inline]
    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        self.scope.configure(cx)
    }
}

/// A proxy object for configuration of a scope.
#[derive(Debug)]
pub struct Context<'a> {
    cx: &'a mut AppContext,
    id: ScopeId,
}

impl<'a> Context<'a> {
    pub(super) fn new(cx: &'a mut AppContext, id: ScopeId) -> Self {
        Self { cx, id }
    }

    /// Adds a route into the current scope.
    pub fn add_route<R>(&mut self, route: R) -> Result<()>
    where
        R: Route,
    {
        self.cx.new_route(self.id, route)
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn add_scope(&mut self, scope: impl Scope) -> Result<()> {
        self.cx.new_scope(self.id, scope)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
    }

    /// Register a `Modifier` into the current scope.
    pub fn add_modifier<M>(&mut self, modifier: M)
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.cx.add_modifier(self.id, modifier)
    }

    pub fn set_prefix(&mut self, prefix: Uri) {
        self.cx.set_prefix(self.id, prefix)
    }
}
