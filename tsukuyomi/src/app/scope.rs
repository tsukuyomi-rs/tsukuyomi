//! The definition of `Scope` and its implementors.

#![allow(deprecated)]

use {
    super::{
        builder::AppContext,
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
    },
    crate::{
        common::Never,
        extractor::{Combine, ExtractStatus, Extractor, Func},
        fs::NamedFile,
        handler::{AsyncResult, Handler},
        modifier::Modifier,
        output::{redirect::Redirect, Responder},
        scoped_map::ScopeId,
        uri::Uri,
    },
    futures::{Async, Future, IntoFuture},
    http::{Method, StatusCode},
    indexmap::IndexSet,
    std::{
        borrow::Cow,
        fmt,
        path::{Path, PathBuf},
        sync::Arc,
    },
};

pub use crate::route;

/// A trait representing a set of configurations within the scope.
pub trait Scope {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error>;

    /// Consumes itself and returns a new `Scope` combined with the specified configuration.
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
#[allow(deprecated)]
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
#[allow(deprecated)]
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

#[deprecated(since = "0.4.2", note = "use `Mount` instead.")]
#[allow(deprecated)]
#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Builder<S>
where
    S: Scope,
{
    /// Adds a route into this scope.
    pub fn route(self, route: impl super::route::Route) -> Builder<impl Scope<Error = Error>> {
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
    pub fn fallback(
        self,
        fallback: impl Fallback + Send + Sync + 'static,
    ) -> Builder<impl Scope<Error = S::Error>> {
        self.state(FallbackInstance::from(fallback))
    }

    /// Set the prefix URL of this scope.
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
pub fn mount(prefix: Uri) -> Mount<()> {
    Mount::new((), prefix)
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[derive(Debug)]
pub struct Mount<S: Scope = ()> {
    scope: S,
    prefix: Uri,
}

impl<S> Default for Mount<S>
where
    S: Scope + Default,
{
    fn default() -> Self {
        Self {
            scope: S::default(),
            prefix: Uri::root(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S> Mount<S>
where
    S: Scope,
{
    /// Create a new `Mount` with the specified components.
    pub fn new(scope: S, prefix: Uri) -> Self {
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
        Self { prefix, ..self }
    }
}

#[allow(deprecated)]
impl<S> Scope for Mount<S>
where
    S: Scope,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        cx.add_scope(raw(move |cx| -> super::Result<()> {
            cx.set_prefix(self.prefix)?;
            self.scope.configure(cx).map_err(Into::into)?;
            Ok(())
        }))
    }
}

/// A type representing the contextual information in `Scope::configure`.
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
    #[deprecated(
        since = "0.4.2",
        note = "This method will be removed in the next version."
    )]
    #[allow(deprecated)]
    pub fn add_route<R>(&mut self, route: R) -> Result<()>
    where
        R: super::route::Route,
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
    #[deprecated(
        since = "0.4.2",
        note = "this method will be removed in the next version."
    )]
    pub fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
    }

    #[deprecated(
        since = "0.4.2",
        note = "this method will be removed in the next version."
    )]
    pub fn add_modifier<M>(&mut self, modifier: M)
    where
        M: Modifier + Send + Sync + 'static,
    {
        self.cx.add_modifier(modifier, self.id)
    }

    #[deprecated(
        since = "0.4.2",
        note = "this method will be removed in the next version."
    )]
    pub fn set_prefix(&mut self, prefix: Uri) -> super::Result<()> {
        self.cx.set_prefix(self.id, prefix)
    }
}

/// Creates a `Route` for building a `Scope` that registers a route within the scope.
pub fn route() -> Route<()> {
    Route::<()>::default()
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug)]
pub struct Route<E: Extractor = (), M: Modifier = ()> {
    extractor: E,
    modifier: M,
    methods: IndexSet<Method>,
    uri: Uri,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            extractor: (),
            modifier: (),
            methods: IndexSet::new(),
            uri: Uri::root(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E, M> Route<E, M>
where
    E: Extractor,
    M: Modifier + Send + Sync + 'static,
{
    /// Sets the URI of this route.
    pub fn uri(self, uri: Uri) -> Self {
        Self { uri, ..self }
    }

    /// Sets the method of this route.
    pub fn method(self, method: Method) -> Self {
        Self {
            methods: {
                let mut methods = self.methods;
                methods.insert(method);
                methods
            },
            ..self
        }
    }

    /// Sets the HTTP methods of this route.
    pub fn methods<I>(self, methods: I) -> Self
    where
        I: IntoIterator<Item = Method>,
    {
        Self {
            methods: {
                let mut orig_methods = self.methods;
                orig_methods.extend(methods);
                orig_methods
            },
            ..self
        }
    }

    /// Appends an `Extractor` to this builder.
    pub fn extract<U>(
        self,
        other: U,
    ) -> Route<
        impl Extractor<Output = <E::Output as Combine<U::Output>>::Out, Error = crate::Error>,
        M,
    >
    where
        U: Extractor,
        E::Output: Combine<U::Output> + Send + 'static,
        U::Output: Send + 'static,
    {
        Route {
            extractor: self
                .extractor
                .into_builder() //
                .and(other)
                .into_inner(),
            modifier: self.modifier,
            methods: self.methods,
            uri: self.uri,
        }
    }

    /// Appends a `Modifier` to this builder.
    pub fn modify<M2>(self, modifier: M2) -> Route<E, crate::modifier::Chain<M, M2>>
    where
        M2: Modifier + Send + Sync + 'static,
    {
        Route {
            extractor: self.extractor,
            modifier: self.modifier.chain(modifier),
            methods: self.methods,
            uri: self.uri,
        }
    }

    fn finish<F, H, R>(
        self,
        f: F,
    ) -> impl super::route::Route<Error = R> + Scope<Error = super::Error>
    where
        F: FnOnce(E) -> std::result::Result<H, R>,
        H: Handler + Send + Sync + 'static,
        R: Into<super::Error>,
    {
        #[allow(missing_debug_implementations)]
        struct Raw<F>(F);

        impl<F, E> super::route::Route for Raw<F>
        where
            F: FnOnce(&mut super::route::Context) -> std::result::Result<(), E>,
            E: Into<super::Error>,
        {
            type Error = E;

            fn configure(
                self,
                cx: &mut super::route::Context,
            ) -> std::result::Result<(), Self::Error> {
                (self.0)(cx)
            }
        }

        impl<F, E> Scope for Raw<F>
        where
            F: FnOnce(&mut super::route::Context) -> std::result::Result<(), E>,
            E: Into<super::Error>,
        {
            type Error = super::Error;

            fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
                cx.add_route(self)
            }
        }

        Raw(move |cx: &mut super::route::Context| {
            let handler = f(self.extractor)?;
            let modifier = self.modifier;
            cx.methods(self.methods);
            cx.uri(self.uri);
            cx.handler(crate::handler::raw(move || {
                modifier.modify(handler.handle())
            }));
            Ok(())
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value of `Responder`.
    #[allow(deprecated)]
    pub fn reply<F>(
        self,
        f: F,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);

            Ok(crate::handler::raw(move || {
                enum Status<F> {
                    Init,
                    InFlight(F),
                }

                let extractor = extractor.clone();
                let f = f.clone();
                let mut status: Status<E::Future> = Status::Init;

                AsyncResult::poll_fn(move |input| loop {
                    status = match status {
                        Status::InFlight(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            return crate::output::internal::respond_to(f.call(arg), input)
                                .map(Async::Ready);
                        }
                        Status::Init => match extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                return crate::output::internal::respond_to(f.call(arg), input)
                                    .map(Async::Ready);
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::InFlight(future),
                        },
                    }
                })
            }))
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The result of provided function is returned by `Future`.
    #[allow(deprecated)]
    pub fn call<F, R>(
        self,
        f: F,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = crate::Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);
            Ok(crate::handler::raw(move || {
                enum Status<F1, F2> {
                    Init,
                    First(F1),
                    Second(F2),
                }

                let extractor = extractor.clone();
                let f = f.clone();
                let mut status: Status<E::Future, R::Future> = Status::Init;

                AsyncResult::poll_fn(move |input| loop {
                    status = match status {
                        Status::First(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            Status::Second(f.call(arg).into_future())
                        }
                        Status::Second(ref mut future) => {
                            let x = futures::try_ready!(input.with_set_current(|| future.poll()));
                            return crate::output::internal::respond_to(x, input).map(Async::Ready);
                        }
                        Status::Init => match extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                Status::Second(f.call(arg).into_future())
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::First(future),
                        },
                    };
                })
            }))
        })
    }
}

impl Route<()> {
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(
        self,
        handler: H,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error>
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| Ok(handler))
    }
}

impl<E> Route<E>
where
    E: Extractor<Output = ()>,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<T>(
        self,
        output: T,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error>
    where
        T: Responder + Clone + Send + Sync + 'static,
    {
        self.reply(move || output.clone())
    }

    /// Creates a `Route` that just replies with a redirection response.
    pub fn redirect(
        self,
        location: impl Into<Cow<'static, str>>,
        status: StatusCode,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error> {
        self.say(Redirect::new(status, location))
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> impl super::route::Route<Error = Never> + Scope<Error = super::Error> {
        let path = {
            #[derive(Clone)]
            #[allow(missing_debug_implementations)]
            struct ArcPath(Arc<PathBuf>);
            impl AsRef<Path> for ArcPath {
                fn as_ref(&self) -> &Path {
                    (*self.0).as_ref()
                }
            }
            ArcPath(Arc::new(path.as_ref().to_path_buf()))
        };

        self.call(move || {
            match config {
                Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
                None => NamedFile::open(path.clone()),
            }.map_err(Into::into)
        })
    }
}
