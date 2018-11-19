#[doc(hidden)]
pub use http::Method;
use {
    crate::{
        async_result::AsyncResult,
        error::Error,
        extractor::{Combine, ExtractStatus, Extractor, Func},
        fs::NamedFile,
        output::{Output, Responder},
        uri::Uri,
    },
    either::Either,
    futures::{Async, Future, IntoFuture},
    indexmap::IndexSet,
    std::{
        path::{Path, PathBuf},
        sync::Arc,
    },
};

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    /// Creates an `AsyncResult` which handles the incoming request.
    fn handle(&self) -> AsyncResult<Output>;
}

impl<F, R> Handler for F
where
    F: Fn() -> R,
    R: Into<AsyncResult<Output>>,
{
    #[inline]
    fn handle(&self) -> AsyncResult<Output> {
        (*self)().into()
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self) -> AsyncResult<Output> {
        (**self).handle()
    }
}

impl<L, R> Handler for Either<L, R>
where
    L: Handler,
    R: Handler,
{
    #[inline]
    fn handle(&self) -> AsyncResult<Output> {
        match self {
            Either::Left(ref handler) => handler.handle(),
            Either::Right(ref handler) => handler.handle(),
        }
    }
}

pub(super) fn raw_handler<F, R>(f: F) -> impl Handler
where
    F: Fn() -> R,
    R: Into<AsyncResult<Output>>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Handler for Raw<F>
    where
        F: Fn() -> R,
        R: Into<AsyncResult<Output>>,
    {
        #[inline]
        fn handle(&self) -> AsyncResult<Output> {
            (self.0)().into()
        }
    }

    Raw(f)
}

/// A builder of `Route`.
#[derive(Debug)]
pub struct Builder<E: Extractor = ()> {
    extractor: E,
    methods: IndexSet<Method>,
    uri: Uri,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            extractor: (),
            methods: IndexSet::new(),
            uri: Uri::root(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Builder<E>
where
    E: Extractor,
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
    pub fn with<U>(
        self,
        other: U,
    ) -> Builder<impl Extractor<Output = <E::Output as Combine<U::Output>>::Out, Error = Error>>
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
            methods: self.methods,
            uri: self.uri,
        }
    }

    fn finish<F, H>(self, f: F) -> impl Route
    where
        F: FnOnce(E) -> H,
        H: Handler + Send + Sync + 'static,
    {
        raw(move |cx| {
            let handler = f(self.extractor);
            cx.methods(self.methods);
            cx.uri(self.uri);
            cx.handler(handler);
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The provided handler always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, handler: F) -> impl Route
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);

            raw_handler(move || {
                enum Status<F> {
                    Init,
                    InFlight(F),
                }

                let extractor = extractor.clone();
                let handler = handler.clone();
                let mut status: Status<E::Future> = Status::Init;

                AsyncResult::poll_fn(move |input| loop {
                    status = match status {
                        Status::InFlight(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            return crate::output::internal::respond_to(handler.call(arg), input)
                                .map(Async::Ready);
                        }
                        Status::Init => match extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                return crate::output::internal::respond_to(
                                    handler.call(arg),
                                    input,
                                ).map(Async::Ready);
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::InFlight(future),
                        },
                    }
                })
            })
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified handler function.
    ///
    /// The result of provided handler is returned by `Future`.
    pub fn handle<F, R>(self, handler: F) -> impl Route
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        self.finish(move |extractor| {
            let extractor = std::sync::Arc::new(extractor);

            raw_handler(move || {
                enum Status<F1, F2> {
                    Init,
                    First(F1),
                    Second(F2),
                }

                let extractor = extractor.clone();
                let handler = handler.clone();
                let mut status: Status<E::Future, R::Future> = Status::Init;

                AsyncResult::poll_fn(move |input| loop {
                    status = match status {
                        Status::First(ref mut future) => {
                            let arg = futures::try_ready!(
                                input.with_set_current(|| future.poll().map_err(Into::into))
                            );
                            Status::Second(handler.call(arg).into_future())
                        }
                        Status::Second(ref mut future) => {
                            let x = futures::try_ready!(input.with_set_current(|| future.poll()));
                            return crate::output::internal::respond_to(x, input).map(Async::Ready);
                        }
                        Status::Init => match extractor.extract(input) {
                            Err(e) => return Err(e.into()),
                            Ok(ExtractStatus::Canceled(output)) => return Ok(Async::Ready(output)),
                            Ok(ExtractStatus::Ready(arg)) => {
                                Status::Second(handler.call(arg).into_future())
                            }
                            Ok(ExtractStatus::Pending(future)) => Status::First(future),
                        },
                    };
                })
            })
        })
    }
}

impl Builder<()> {
    pub fn raw<H>(self, handler: H) -> impl Route
    where
        H: Handler + Send + Sync + 'static,
    {
        self.finish(move |()| handler)
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()>,
{
    pub fn serve_file<P>(self, path: P) -> ServeFile<E, P>
    where
        P: AsRef<Path>,
    {
        ServeFile {
            builder: self,
            path,
            config: None,
        }
    }
}

#[derive(Debug)]
pub struct ServeFile<E, P>
where
    E: Extractor<Output = ()>,
    P: AsRef<Path>,
{
    builder: Builder<E>,
    path: P,
    config: Option<crate::fs::OpenConfig>,
}

impl<E, P> ServeFile<E, P>
where
    E: Extractor<Output = ()>,
    P: AsRef<Path>,
{
    pub fn open_config(self, config: crate::fs::OpenConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }
}

impl<E, P> Route for ServeFile<E, P>
where
    E: Extractor<Output = ()>,
    P: AsRef<Path>,
{
    fn configure(self, cx: &mut Context) {
        #[derive(Clone)]
        #[allow(missing_debug_implementations)]
        struct ArcPath(Arc<PathBuf>);

        impl AsRef<Path> for ArcPath {
            fn as_ref(&self) -> &Path {
                (*self.0).as_ref()
            }
        }

        let path = ArcPath(Arc::new(self.path.as_ref().to_path_buf()));
        let config = self.config;

        self.builder
            .handle(move || {
                match config {
                    Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
                    None => NamedFile::open(path.clone()),
                }.map_err(Into::into)
            }).configure(cx);
    }
}

pub trait Route {
    fn configure(self, cx: &mut Context);
}

fn raw<F>(f: F) -> impl Route
where
    F: FnOnce(&mut Context),
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F> Route for Raw<F>
    where
        F: FnOnce(&mut Context),
    {
        fn configure(self, cx: &mut Context) {
            (self.0)(cx)
        }
    }

    Raw(f)
}

#[allow(missing_debug_implementations)]
pub struct Context {
    pub(super) uri: Uri,
    pub(super) methods: Option<IndexSet<Method>>,
    pub(super) handler: Option<Box<dyn Handler + Send + Sync + 'static>>,
}

impl Context {
    fn uri(&mut self, uri: Uri) {
        self.uri = uri;
    }

    fn methods<I>(&mut self, methods: I)
    where
        I: IntoIterator<Item = Method>,
    {
        self.methods = Some(methods.into_iter().collect());
    }

    fn handler<H>(&mut self, handler: H)
    where
        H: Handler + Send + Sync + 'static,
    {
        self.handler = Some(Box::new(handler));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> Builder<impl Extractor<Output = (u32, String)>> {
        crate::app::route()
            .uri("/:id/:name".parse().unwrap())
            .with(crate::extractor::param::pos(0))
            .with(crate::extractor::param::pos(1))
    }

    #[test]
    #[ignore]
    fn compiletest1() {
        drop(
            crate::app()
                .route(
                    generated() //
                        .reply(|id: u32, name: String| {
                            drop((id, name));
                            "dummy"
                        }),
                ) //
                .build()
                .expect("failed to construct App"),
        );
    }

    #[test]
    #[ignore]
    fn compiletest2() {
        drop(
            crate::app()
                .route(
                    generated() //
                        .with(crate::extractor::body::plain())
                        .reply(|id: u32, name: String, body: String| {
                            drop((id, name, body));
                            "dummy"
                        }),
                ) //
                .build()
                .expect("failed to construct App"),
        );
    }
}
