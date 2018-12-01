#![allow(deprecated)]
#![doc(hidden)]
#![deprecated(
    since = "0.4.2",
    note = "the module `app::route` will be removed in the next version."
)]

#[doc(hidden)]
pub use http::Method;
use {
    super::Uri,
    crate::{
        error::Error,
        extractor::{Combine, ExtractStatus, Extractor, Func},
        fs::NamedFile,
        handler::{raw as raw_handler, AsyncResult, Handler},
        modifier::{Chain, Modifier},
        output::{redirect::Redirect, Responder},
        Never,
    },
    futures::{Async, Future, IntoFuture},
    http::StatusCode,
    indexmap::IndexSet,
    std::{
        borrow::Cow,
        fmt,
        path::{Path, PathBuf},
        sync::Arc,
    },
};

/// A builder of `Route`.
pub struct Builder<E: Extractor = (), M: Modifier = ()> {
    extractor: E,
    modifier: M,
    methods: IndexSet<Method>,
    uri: Uri,
}

impl<E: Extractor + fmt::Debug, M: Modifier + fmt::Debug> fmt::Debug for Builder<E, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("extractor", &self.extractor)
            .field("modifier", &self.modifier)
            .field("methods", &self.methods)
            .field("uri", &self.uri)
            .finish()
    }
}

impl Default for Builder {
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
impl<E, M> Builder<E, M>
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
    ) -> Builder<impl Extractor<Output = <E::Output as Combine<U::Output>>::Out, Error = Error>, M>
    where
        U: Extractor,
        E::Output: Combine<U::Output> + Send + 'static,
        U::Output: Send + 'static,
    {
        Builder {
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
    pub fn modify<M2>(self, modifier: M2) -> Builder<E, Chain<M, M2>>
    where
        M2: Modifier + Send + Sync + 'static,
    {
        Builder {
            extractor: self.extractor,
            modifier: self.modifier.chain(modifier),
            methods: self.methods,
            uri: self.uri,
        }
    }

    fn finish<F, H, R>(self, f: F) -> impl Route<Error = R>
    where
        F: FnOnce(E) -> Result<H, R>,
        H: Handler + Send + Sync + 'static,
        R: Into<super::Error>,
    {
        raw(move |cx| {
            let handler = f(self.extractor)?;
            let modifier = self.modifier;
            cx.methods(self.methods);
            cx.uri(self.uri);
            cx.handler(raw_handler(move || modifier.modify(handler.handle())));
            Ok(())
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, f: F) -> impl Route<Error = Never>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);

            Ok(raw_handler(move || {
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
    pub fn call<F, R>(self, f: F) -> impl Route<Error = Never>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);
            Ok(raw_handler(move || {
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

impl Builder<()> {
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> impl Route<Error = Never>
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| Ok(handler))
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()>,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<T>(self, output: T) -> impl Route<Error = Never>
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
    ) -> impl Route<Error = Never> {
        self.say(Redirect::new(status, location))
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> impl Route<Error = Never> {
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

/// A trait representing the types for constructing a route in `App`.
pub trait Route {
    type Error: Into<super::Error>;

    fn configure(self, cx: &mut Context) -> Result<(), Self::Error>;
}

fn raw<F, E>(f: F) -> impl Route<Error = E>
where
    F: FnOnce(&mut Context) -> Result<(), E>,
    E: Into<super::Error>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, E> Route for Raw<F>
    where
        F: FnOnce(&mut Context) -> Result<(), E>,
        E: Into<super::Error>,
    {
        type Error = E;

        fn configure(self, cx: &mut Context) -> Result<(), Self::Error> {
            (self.0)(cx)
        }
    }

    Raw(f)
}

#[deprecated(
    since = "0.4.2",
    note = "the trait Route will be removed in the next version."
)]
#[allow(missing_debug_implementations)]
pub struct Context {
    pub(super) uri: Uri,
    pub(super) methods: Option<IndexSet<Method>>,
    pub(super) handler: Option<Box<dyn Handler + Send + Sync + 'static>>,
}

impl Context {
    pub(super) fn uri(&mut self, uri: Uri) {
        self.uri = uri;
    }

    pub(super) fn methods<I>(&mut self, methods: I)
    where
        I: IntoIterator<Item = Method>,
    {
        self.methods = Some(methods.into_iter().collect());
    }

    pub(super) fn handler<H>(&mut self, handler: H)
    where
        H: Handler + Send + Sync + 'static,
    {
        self.handler = Some(Box::new(handler));
    }
}
