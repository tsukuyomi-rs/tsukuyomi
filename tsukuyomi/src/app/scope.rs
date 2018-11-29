use {
    super::{
        builder::AppContext,
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
    },
    crate::{common::Never, modifier::Modifier, scoped_map::ScopeId, uri::Uri},
    std::fmt,
};

#[allow(deprecated)]
use super::route::Route;

pub trait Scope {
    type Error: Into<Error>;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error>;

    fn chain<S>(self, next: S) -> Chain<Self, S>
    where
        Self: Sized,
        S: Scope,
    {
        Chain::new(self, next)
    }
}

impl Scope for () {
    type Error = Never;

    fn configure(self, _: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

/// Creates a `Scope` that registers the specified state to be shared into the scope.
pub fn state<T>(state: T) -> impl Scope<Error = Never>
where
    T: Send + Sync + 'static,
{
    self::raw(move |cx| {
        cx.set_state(state);
        Ok(())
    })
}

/// Creates a `Scope` that registers the specified `Modifier` into the scope.
pub fn modifier<M>(modifier: M) -> impl Scope<Error = Never>
where
    M: Modifier + Send + Sync + 'static,
{
    self::raw(move |cx| {
        cx.add_modifier(modifier);
        Ok(())
    })
}

/// Creates a `Scope` that registers the specified `Fallback` into the scope.
pub fn fallback<F>(fallback: F) -> impl Scope<Error = Never>
where
    F: Fallback + Send + Sync + 'static,
{
    state(FallbackInstance::from(fallback))
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

#[deprecated(since = "0.4.2", note = "use `Mount` instead.")]
pub struct Builder<S: Scope = ()> {
    pub(super) scope: S,
}

#[allow(deprecated)]
impl<S: fmt::Debug + Scope> fmt::Debug for Builder<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("scope", &self.scope)
            .finish()
    }
}

#[allow(deprecated)]
impl<S: Default + Scope> Default for Builder<S> {
    fn default() -> Self {
        Self {
            scope: S::default(),
        }
    }
}

#[allow(deprecated)]
#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    /// Adds a route into this scope.
    #[deprecated(since = "0.4.1", note = "use Builder::with(route) instead.")]
    #[allow(deprecated)]
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_route(route)?;
                Ok(())
            }),
        }
    }

    /// Create a new subscope onto this scope.
    #[inline]
    pub fn mount(self, new_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                cx.add_scope(new_scope)?;
                Ok(())
            }),
        }
    }

    /// Merges the specified `Scope` into this scope, *without* creating a new subscope.
    pub fn with(self, next_scope: impl Scope) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx).map_err(Into::into)?;
                next_scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }

    /// Registers a shared variable into this scope.
    #[deprecated(
        since = "0.4.1",
        note = "use Builder::with(state(scope)) instead"
    )]
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

    /// Registers a `Modifier` into this scope.
    #[deprecated(
        since = "0.4.1",
        note = "use Builder::with(modifier(scope)) instead"
    )]
    pub fn modifier(
        self,
        modifier: impl Modifier + Send + Sync + 'static,
    ) -> Builder<impl Scope<Error = S::Error>> {
        Builder {
            scope: raw(move |cx| {
                self.scope.configure(cx)?;
                cx.add_modifier(modifier);
                Ok(())
            }),
        }
    }

    /// Registers a `Fallback` into this scope.
    #[deprecated(
        since = "0.4.1",
        note = "use Builder::with(fallback(scope)) instead"
    )]
    #[allow(deprecated)]
    pub fn fallback(
        self,
        fallback: impl Fallback + Send + Sync + 'static,
    ) -> Builder<impl Scope<Error = S::Error>> {
        self.state(FallbackInstance::from(fallback))
    }

    /// Set the prefix URL of this scope.
    #[deprecated(
        since = "0.4.1",
        note = "this method will be removed in the next version."
    )]
    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = Error>> {
        Builder {
            scope: raw(move |cx| {
                cx.set_prefix(prefix)?;
                self.scope.configure(cx).map_err(Into::into)?;
                Ok(())
            }),
        }
    }
}

#[allow(deprecated)]
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

/// A pair representing a chain of `Scope`.
#[derive(Debug)]
pub struct Chain<S1, S2>(S1, S2);

impl<S1, S2> Chain<S1, S2>
where
    S1: Scope,
    S2: Scope,
{
    /// Create a new `Chain` from the specified `Scope`s.
    pub fn new(s1: S1, s2: S2) -> Self {
        Chain(s1, s2)
    }
}

impl<S1, S2> Scope for Chain<S1, S2>
where
    S1: Scope,
    S2: Scope,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        self.0.configure(cx).map_err(Into::into)?;
        self.1.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

/// A function that creates a `Mount` with the empty scope items.
pub fn mount() -> Mount<()> {
    Mount::new((), None)
}

/// An instance of `Scope` that represents a scope with a specific prefix.
#[derive(Debug, Default)]
pub struct Mount<S: Scope = ()> {
    scope: S,
    prefix: Option<Uri>,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Mount<S>
where
    S: Scope,
{
    /// Create a new `Mount` with the specified components.
    pub fn new(scope: S, prefix: Option<Uri>) -> Self {
        Mount { scope, prefix }
    }

    /// Merges the specified `Scope` into the inner scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Mount<Chain<S, S2>>
    where
        S2: Scope,
    {
        Mount {
            scope: Chain::new(self.scope, next_scope),
            prefix: self.prefix,
        }
    }

    /// Replaces the inner `Scope` with the specified value.
    pub fn scope<S2>(self, scope: S2) -> Mount<S2>
    where
        S2: Scope,
    {
        Mount {
            scope,
            prefix: self.prefix,
        }
    }

    /// Sets the prefix of the URL appended to the all routes in the inner scope.
    pub fn prefix(self, prefix: Uri) -> Self {
        Self {
            prefix: Some(prefix),
            ..self
        }
    }
}

impl<S> Scope for Mount<S>
where
    S: Scope,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        cx.add_scope(raw(move |cx| -> super::Result<()> {
            if let Some(prefix) = self.prefix {
                cx.set_prefix(prefix)?;
            }
            self.scope.configure(cx).map_err(Into::into)?;
            Ok(())
        }))
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
    // note:
    // Currently, this method is only called in `fs::Staticfiles`
    // to add routes. In order to provide the implementors of `Scope`
    // that adds some route(s) dynamically, the context need to provide
    // the similar API.
    #[deprecated(
        since = "0.4.1",
        note = "This method will be removed in the next version."
    )]
    #[allow(deprecated)]
    pub fn add_route<R>(&mut self, route: R) -> Result<()>
    where
        R: Route,
    {
        self.cx.new_route(self.id, route)
    }

    /// Create a new scope mounted to the certain URI.
    #[inline]
    pub(super) fn add_scope<S>(&mut self, new_scope: S) -> Result<()>
    where
        S: Scope,
    {
        self.cx.new_scope(self.id, new_scope)
    }

    /// Adds a *scope-local* variable into the application.
    pub fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
    }

    pub fn add_modifier<M>(&mut self, modifier: M)
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.cx.add_modifier(modifier, self.id)
    }

    pub fn set_prefix(&mut self, prefix: Uri) -> super::Result<()> {
        self.cx.set_prefix(self.id, prefix)
    }
}
