//! The definition of `Scope` and its implementors.

#![allow(deprecated)]

use {
    super::{
        builder::AppContext,
        error::{Error, Result},
        fallback::{Fallback, FallbackInstance},
        scoped_map::ScopeId,
        Uri,
    },
    crate::{
        common::{Never, TryFrom},
        extractor::{Combine, ExtractStatus, Extractor, Func},
        fs::NamedFile,
        handler::{AsyncResult, Handler},
        modifier::Modifier,
        output::{redirect::Redirect, Responder},
    },
    futures::{Async, Future, IntoFuture},
    http::{HttpTryFrom, Method, StatusCode},
    indexmap::{indexset, IndexSet},
    std::{
        borrow::Cow,
        fmt,
        path::{Path, PathBuf},
        sync::Arc,
    },
};

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

#[doc(hidden)]
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
pub fn mount<T>(prefix: T) -> Result<Mount<()>>
where
    Uri: TryFrom<T>,
{
    Ok(Mount::new((), Uri::try_from(prefix)?))
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
    pub fn prefix<T>(self, prefix: T) -> Result<Self>
    where
        Uri: TryFrom<T>,
    {
        Ok(Self {
            prefix: Uri::try_from(prefix)?,
            ..self
        })
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

    #[doc(hidden)]
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

    #[doc(hidden)]
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

    #[doc(hidden)]
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

    #[doc(hidden)]
    #[deprecated(
        since = "0.4.2",
        note = "this method will be removed in the next version."
    )]
    pub fn set_prefix(&mut self, prefix: Uri) -> super::Result<()> {
        self.cx.set_prefix(self.id, prefix)
    }
}

// ==== Route ====

/// A set of request methods that a route accepts.
#[derive(Debug)]
pub struct Methods(IndexSet<Method>);

impl TryFrom<Self> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> std::result::Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> std::result::Result<Self, Self::Error> {
        Ok(Methods(indexset! { method }))
    }
}

impl<M> TryFrom<Vec<M>> for Methods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> std::result::Result<Self, Self::Error> {
        let methods = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)?;
        Ok(Methods(methods))
    }
}

impl<'a> TryFrom<&'a str> for Methods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> std::result::Result<Self, Self::Error> {
        let methods = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(Methods(methods))
    }
}

/// Creates a `Route` for building a `Scope` that registers a route within the scope.
pub fn route<T>(uri: T) -> Result<Route<(), ()>>
where
    Uri: TryFrom<T>,
{
    let uri = Uri::try_from(uri)?;
    Ok(Route::new((), (), uri))
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug)]
pub struct Route<E: Extractor = (), M: Modifier = ()> {
    extractor: E,
    modifier: M,
    uri: Uri,
    methods: Methods,
}

impl Default for Route {
    fn default() -> Self {
        Self::new((), (), Uri::root())
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E, M> Route<E, M>
where
    E: Extractor,
    M: Modifier + Send + Sync + 'static,
{
    pub fn new(extractor: E, modifier: M, uri: Uri) -> Self {
        Self {
            extractor,
            modifier,
            uri,
            methods: Methods(IndexSet::new()),
        }
    }

    /// Sets the URI of this route.
    pub fn uri<T>(self, uri: T) -> Result<Self>
    where
        Uri: TryFrom<T>,
    {
        Ok(Self {
            uri: Uri::try_from(uri)?,
            ..self
        })
    }

    /// Sets the HTTP methods that this route accepts.
    pub fn methods<M2>(self, methods: M2) -> Result<Self>
    where
        Methods: TryFrom<M2>,
    {
        Ok(Self {
            methods: Methods::try_from(methods).map_err(Into::into)?,
            ..self
        })
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
            uri: self.uri,
            methods: self.methods,
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
            uri: self.uri,
            methods: self.methods,
        }
    }

    fn finish<F, H, R>(self, f: F) -> impl Scope<Error = super::Error>
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

        Raw(move |cx: &mut super::route::Context| -> super::Result<_> {
            let handler = f(self.extractor).map_err(Into::into)?;
            let modifier = self.modifier;
            cx.methods(self.methods.0);
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
    pub fn reply<F>(self, f: F) -> impl Scope<Error = super::Error>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| -> super::Result<_> {
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
    pub fn call<F, R>(self, f: F) -> impl Scope<Error = super::Error>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = crate::Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| -> super::Result<_> {
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

impl<M> Route<(), M>
where
    M: Modifier + Send + Sync + 'static,
{
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> impl Scope<Error = super::Error>
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| -> super::Result<_> { Ok(handler) })
    }
}

impl<E, M> Route<E, M>
where
    E: Extractor<Output = ()>,
    M: Modifier + Send + Sync + 'static,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<T>(self, output: T) -> impl Scope<Error = super::Error>
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
    ) -> impl Scope<Error = super::Error> {
        self.say(Redirect::new(status, location))
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> impl Scope<Error = super::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_methods_try_from() {
        assert_eq!(
            Methods::try_from(Methods(indexset! { Method::GET }))
                .unwrap()
                .0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(Method::GET).unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(vec![Method::GET, Method::POST])
                .unwrap()
                .0,
            indexset! { Method::GET, Method::POST }
        );
        assert_eq!(
            Methods::try_from("GET").unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from("GET, POST").unwrap().0,
            indexset! { Method::GET , Method::POST }
        );
        assert!(Methods::try_from("").is_err());
    }
}
