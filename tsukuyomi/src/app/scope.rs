//! The definition of `Scope` and its implementors.

use {
    super::{
        builder::AppContext,
        error::{Error, Result},
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
    Ok(Mount::new((), (), Uri::try_from(prefix)?))
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[derive(Debug)]
pub struct Mount<S: Scope = (), M: Modifier = ()> {
    scope: S,
    modifier: M,
    prefix: Uri,
}

impl<S, M> Default for Mount<S, M>
where
    S: Scope + Default,
    M: Modifier + Default,
{
    fn default() -> Self {
        Self {
            scope: S::default(),
            modifier: M::default(),
            prefix: Uri::root(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> Mount<S, M>
where
    S: Scope,
    M: Modifier,
{
    /// Create a new `Mount` with the specified components.
    pub fn new(scope: S, modifier: M, prefix: Uri) -> Self {
        Mount {
            scope,
            modifier,
            prefix,
        }
    }

    /// Merges the specified `Scope` into the inner scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Mount<Chain<S, S2>, M>
    where
        S2: Scope,
    {
        Mount {
            scope: Chain::new(self.scope, next_scope),
            modifier: self.modifier,
            prefix: self.prefix,
        }
    }

    /// Replaces the inner `Scope` with the specified value.
    pub fn scope<S2>(self, scope: S2) -> Mount<S2, M>
    where
        S2: Scope,
    {
        Mount {
            scope,
            modifier: self.modifier,
            prefix: self.prefix,
        }
    }

    pub fn modifier<M2>(self, modifier: M2) -> Mount<S, crate::modifier::Chain<M, M2>>
    where
        M2: Modifier,
    {
        Mount {
            scope: self.scope,
            modifier: crate::modifier::Chain::new(self.modifier, modifier),
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

impl<S, M> Scope for Mount<S, M>
where
    S: Scope,
    M: Modifier + Send + Sync + 'static,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_>) -> std::result::Result<(), Self::Error> {
        cx.cx
            .new_scope(cx.id, self.prefix, self.modifier, self.scope)
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

    pub(super) fn set_state<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cx.set_state(value, self.id)
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
        raw(move |cx: &mut Context<'_>| -> super::Result<_> {
            let handler = f(self.extractor).map_err(Into::into)?;
            let modifier = self.modifier;
            let handler = crate::handler::raw(move || modifier.modify(handler.handle()));
            cx.cx.new_route(cx.id, self.uri, self.methods.0, handler)
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value of `Responder`.
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
