//! The definition of `Scope` and its implementors.

use {
    super::{
        builder::{AppContext, ScopeData},
        error::{Error, Result},
        fallback::Fallback,
        Uri,
    },
    crate::{
        common::{Chain, Never, TryFrom},
        extractor::{Combine, ExtractStatus, Extractor, Func},
        fs::NamedFile,
        handler::{AsyncResult, Handler},
        modifier::Modifier,
        output::{redirect::Redirect, Output, Responder},
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
pub trait Scope<M> {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Context<'_, M>) -> std::result::Result<(), Self::Error>;

    /// Consumes itself and returns a new `Scope` combined with the specified configuration.
    fn chain<S>(self, next: S) -> Chain<Self, S>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct Context<'a, M> {
    pub(super) app: &'a mut AppContext,
    pub(super) data: &'a ScopeData,
    pub(super) modifier: M,
}

impl<M> Scope<M> for () {
    type Error = Never;

    fn configure(self, _: &mut Context<'_, M>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

impl<S1, S2, M> Scope<M> for Chain<S1, S2>
where
    S1: Scope<M>,
    S2: Scope<M>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M>) -> std::result::Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

/// A function that creates a `Mount` with the empty scope items.
pub fn mount<T>(prefix: T) -> Result<Mount<(), ()>>
where
    Uri: TryFrom<T>,
{
    Ok(Mount::new((), (), Uri::try_from(prefix)?))
}

/// An instance of `Scope` that represents a sub-scope with a specific prefix.
#[allow(missing_debug_implementations)]
pub struct Mount<S = (), M = ()> {
    scope: S,
    modifier: M,
    fallback: Option<Box<dyn Fallback + Send + Sync + 'static>>,
    prefix: Uri,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> Mount<S, M> {
    /// Create a new `Mount` with the specified components.
    pub fn new(scope: S, modifier: M, prefix: Uri) -> Self {
        Mount {
            scope,
            modifier,
            fallback: None,
            prefix,
        }
    }

    /// Merges the specified `Scope` into the inner scope, *without* creating a new subscope.
    pub fn with<S2>(self, next_scope: S2) -> Mount<Chain<S, S2>, M> {
        Mount {
            scope: Chain::new(self.scope, next_scope),
            modifier: self.modifier,
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn modifier<M2>(self, modifier: M2) -> Mount<S, Chain<M, M2>> {
        Mount {
            scope: self.scope,
            modifier: Chain::new(self.modifier, modifier),
            fallback: self.fallback,
            prefix: self.prefix,
        }
    }

    pub fn fallback<F>(self, fallback: F) -> Self
    where
        F: Fallback + Send + Sync + 'static,
    {
        Self {
            fallback: Some(Box::new(fallback)),
            ..self
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

impl<S, M1, M2> Scope<M1> for Mount<S, M2>
where
    M1: Clone,
    S: Scope<Chain<M1, M2>>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M1>) -> std::result::Result<(), Self::Error> {
        cx.app.new_scope(
            &cx.data,
            self.prefix,
            Chain::new(cx.modifier.clone(), self.modifier),
            self.fallback,
            self.scope,
        )
    }
}

// ==== Route ====

/// A set of request methods that a route accepts.
#[derive(Debug, Default)]
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
#[derive(Debug, Default)]
pub struct Route<E: Extractor = (), M = ()> {
    extractor: E,
    modifier: M,
    uri: Uri,
    methods: Methods,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E, M> Route<E, M>
where
    E: Extractor,
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
    pub fn modify<M2>(self, modifier: M2) -> Route<E, Chain<M, M2>> {
        Route {
            extractor: self.extractor,
            modifier: Chain::new(self.modifier, modifier),
            uri: self.uri,
            methods: self.methods,
        }
    }

    pub fn finish<F>(self, finalizer: F) -> Endpoint<E, M, F>
    where
        F: Finalizer<E>,
    {
        Endpoint {
            extractor: self.extractor,
            finalizer,
            modifier: self.modifier,
            methods: self.methods,
            uri: self.uri,
        }
    }
}

pub trait Finalizer<E> {
    type Handler: Handler;
    fn finalize(self, extractor: E) -> super::Result<Self::Handler>;
}

#[derive(Debug)]
pub struct Endpoint<E, M, F> {
    extractor: E,
    modifier: M,
    methods: Methods,
    uri: Uri,
    finalizer: F,
}

impl<E, F, M1, M2> Scope<M1> for Endpoint<E, M2, F>
where
    E: Extractor,
    F: Finalizer<E>,
    M2: Modifier<F::Handler>,
    M1: Modifier<M2::Out>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M1>) -> std::result::Result<(), Self::Error> {
        let handler = self.finalizer.finalize(self.extractor)?;
        let handler = cx.modifier.modify(self.modifier.modify(handler));
        cx.app
            .new_route(&cx.data, self.uri, self.methods.0, handler)
    }
}

impl<E, M> Route<E, M>
where
    E: Extractor,
{
    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, f: F) -> Endpoint<E, M, impl Finalizer<E>>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        #[allow(missing_debug_implementations)]
        struct ReplyFuture<E, F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            extractor: Arc<E>,
            f: F,
            status: Status<E::Future>,
        }

        enum Status<F> {
            Init,
            InFlight(F),
        }

        impl<E, F> AsyncResult<Output> for ReplyFuture<E, F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            fn poll_ready(
                &mut self,
                input: &mut crate::input::Input<'_>,
            ) -> futures::Poll<Output, crate::Error> {
                loop {
                    self.status = match self.status {
                        Status::InFlight(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            return crate::output::internal::respond_to(self.f.call(arg), input)
                                .map(Async::Ready);
                        }
                        Status::Init => match self.extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                return crate::output::internal::respond_to(self.f.call(arg), input)
                                    .map(Async::Ready);
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::InFlight(future),
                        },
                    }
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct ReplyHandler<E, F>(Arc<E>, F);

        impl<E, F> Handler for ReplyHandler<E, F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            type Handle = ReplyFuture<E, F>;

            fn handle(&self) -> Self::Handle {
                ReplyFuture {
                    extractor: self.0.clone(),
                    f: self.1.clone(),
                    status: Status::Init,
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct Reply<F>(F);
        impl<F, E> Finalizer<E> for Reply<F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            type Handler = ReplyHandler<E, F>;
            fn finalize(self, extractor: E) -> super::Result<Self::Handler> {
                Ok(ReplyHandler {
                    0: std::sync::Arc::new(extractor),
                    1: self.0,
                })
            }
        }

        self.finish(Reply(f))
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The result of provided function is returned by `Future`.
    pub fn call<F, R>(self, f: F) -> Endpoint<E, M, impl Finalizer<E>>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = crate::Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        #[allow(missing_debug_implementations)]
        struct CallHandlerFuture<E, F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: IntoFuture<Error = crate::Error>,
            <F::Out as IntoFuture>::Future: Send + 'static,
            <F::Out as IntoFuture>::Item: Responder,
        {
            extractor: Arc<E>,
            f: F,
            status: Status<E::Future, <F::Out as IntoFuture>::Future>,
        }

        enum Status<F1, F2> {
            Init,
            First(F1),
            Second(F2),
        }

        impl<E, F, R> AsyncResult<Output> for CallHandlerFuture<E, F>
        where
            E: Extractor,
            F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
            R: IntoFuture<Error = crate::Error>,
            R::Future: Send + 'static,
            R::Item: Responder,
        {
            fn poll_ready(
                &mut self,
                input: &mut crate::input::Input<'_>,
            ) -> futures::Poll<Output, crate::Error> {
                loop {
                    self.status = match self.status {
                        Status::First(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            Status::Second(self.f.call(arg).into_future())
                        }
                        Status::Second(ref mut future) => {
                            let x = futures::try_ready!(input.with_set_current(|| future.poll()));
                            return crate::output::internal::respond_to(x, input).map(Async::Ready);
                        }
                        Status::Init => match self.extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                Status::Second(self.f.call(arg).into_future())
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::First(future),
                        },
                    };
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct CallHandler<E, F>(Arc<E>, F);
        impl<E, F, R> Handler for CallHandler<E, F>
        where
            E: Extractor,
            F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
            R: IntoFuture<Error = crate::Error>,
            R::Future: Send + 'static,
            R::Item: Responder,
        {
            type Handle = CallHandlerFuture<E, F>;

            fn handle(&self) -> Self::Handle {
                CallHandlerFuture {
                    extractor: self.0.clone(),
                    f: self.1.clone(),
                    status: Status::Init,
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct Call<F>(F);
        impl<F, E, R> Finalizer<E> for Call<F>
        where
            E: Extractor,
            F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
            R: IntoFuture<Error = crate::Error>,
            R::Future: Send + 'static,
            R::Item: Responder,
        {
            type Handler = CallHandler<E, F>;
            fn finalize(self, extractor: E) -> super::Result<Self::Handler> {
                Ok(CallHandler {
                    0: std::sync::Arc::new(extractor),
                    1: self.0,
                })
            }
        }

        self.finish(Call(f))
    }
}

impl<M> Route<(), M> {
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> Endpoint<(), M, impl Finalizer<(), Handler = H>>
    where
        H: Handler,
    {
        #[allow(missing_debug_implementations)]
        struct Raw<H>(H);
        impl<H> Finalizer<()> for Raw<H>
        where
            H: Handler,
        {
            type Handler = H;
            fn finalize(self, _: ()) -> super::Result<H> {
                Ok(self.0)
            }
        }

        self.finish(Raw(handler))
    }
}

impl<E, M> Route<E, M>
where
    E: Extractor<Output = ()>,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<T>(self, output: T) -> Endpoint<E, M, impl Finalizer<E>>
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
    ) -> Endpoint<E, M, impl Finalizer<E>> {
        self.say(Redirect::new(status, location))
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> Endpoint<E, M, impl Finalizer<E>> {
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
