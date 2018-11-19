use {
    super::{
        builder::AppContext,
        error::{Error, Result},
        route::Route,
    },
    crate::{
        common::Never,
        modifier::{Chain, Modifier},
        scoped_map::ScopeId,
        uri::Uri,
    },
};

pub trait Scope {
    type Error: Into<Error>;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error>;
}

impl Scope for () {
    type Error = Never;

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
pub struct Builder<S: Scope = (), M = ()> {
    pub(super) scope: S,
    pub(super) modifier: M,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> Builder<S, M>
where
    S: Scope,
    M: Modifier + Send + Sync + 'static,
{
    /// Adds a route into the current scope.
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>, M> {
        let Self { scope, modifier } = self;
        Builder {
            modifier,
            scope: raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.add_route(route)?;
                Ok(())
            }),
        }
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub fn mount<S2, M2>(self, new_scope: Builder<S2, M2>) -> Builder<impl Scope<Error = Error>, M>
    where
        S2: Scope,
        M2: Modifier + Send + Sync + 'static,
    {
        let Self { scope, modifier } = self;
        Builder {
            modifier,
            scope: raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                cx.add_scope(new_scope)?;
                Ok(())
            }),
        }
    }

    /// Merges the specified `Scope` into the current scope, *without* creating a new scope.
    pub fn with(self, next_scope: impl Scope) -> Builder<impl Scope<Error = Error>, M> {
        let Self { scope, modifier } = self;
        Builder {
            modifier,
            scope: raw(move |cx| {
                scope.configure(cx).map_err(Into::into)?;
                next_scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }

    /// Adds a *scope-local* variable into the application.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>, M>
    where
        T: Send + Sync + 'static,
    {
        let Self { scope, modifier } = self;
        Builder {
            modifier,
            scope: raw(move |cx| {
                scope.configure(cx)?;
                cx.set_state(state);
                Ok(())
            }),
        }
    }

    /// Register a `Modifier` into the current scope.
    pub fn modifier<M2>(self, modifier: M2) -> Builder<S, Chain<M, M2>>
    where
        M2: Modifier + Send + Sync + 'static,
    {
        Builder {
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
        }
    }

    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = S::Error>, M> {
        let Self { scope, modifier } = self;
        Builder {
            modifier,
            scope: raw(move |cx| {
                scope.configure(cx)?;
                cx.set_prefix(prefix);
                Ok(())
            }),
        }
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
    fn add_scope(
        &mut self,
        scope: Builder<impl Scope, impl Modifier + Send + Sync + 'static>,
    ) -> Result<()> {
        self.cx.new_scope(self.id, scope.scope, scope.modifier)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
    }

    pub fn set_prefix(&mut self, prefix: Uri) {
        self.cx.set_prefix(self.id, prefix)
    }
}
