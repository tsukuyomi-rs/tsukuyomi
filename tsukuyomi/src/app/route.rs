use {
    super::{
        builder::{Context, Scope},
        uri::Uri,
    },
    crate::{
        common::{Chain, MaybeFuture, Never, TryFrom},
        extractor::{Combine, Extractor, Func, Tuple},
        fs::NamedFile,
        handler::{Handler, MakeHandler},
        input::{Input, Params},
        modifier::Modifier,
        output::{redirect::Redirect, Responder},
    },
    futures::{Future, IntoFuture, Poll},
    http::{HttpTryFrom, Method, StatusCode},
    indexmap::{indexset, IndexSet},
    std::{
        borrow::Cow,
        marker::PhantomData,
        path::{Path, PathBuf},
        str::Utf8Error,
        sync::Arc,
    },
    url::percent_encoding::percent_decode,
};

#[derive(Debug)]
pub struct EncodedStr<'a>(&'a str);

impl<'a> EncodedStr<'a> {
    pub(crate) fn new(s: &'a str) -> Self {
        EncodedStr(s)
    }

    pub fn decode_utf8(&self) -> Result<Cow<'a, str>, Utf8Error> {
        percent_decode(self.0.as_bytes()).decode_utf8()
    }

    pub fn decode_utf8_lossy(&self) -> Cow<'a, str> {
        percent_decode(self.0.as_bytes()).decode_utf8_lossy()
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait FromParam: Sized + Send + 'static {
    type Error: Into<crate::Error>;

    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error>;
}

macro_rules! impl_from_param {
    ($($t:ty),*) => {$(
        impl FromParam for $t {
            type Error = crate::Error;

            #[inline]
            fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
                s.decode_utf8()
                    .map_err(crate::error::bad_request)?
                    .parse()
                    .map_err(crate::error::bad_request)
            }
        }
    )*};
}

impl_from_param!(bool, char, f32, f64, String);
impl_from_param!(i8, i16, i32, i64, i128, isize);
impl_from_param!(u8, u16, u32, u64, u128, usize);
impl_from_param!(
    std::net::SocketAddr,
    std::net::SocketAddrV4,
    std::net::SocketAddrV6,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr,
    url::Url,
    uuid::Uuid
);

impl FromParam for PathBuf {
    type Error = crate::Error;

    #[inline]
    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
        s.decode_utf8()
            .map(|s| Self::from(s.into_owned()))
            .map_err(crate::error::bad_request)
    }
}

/// A set of request methods that a route accepts.
#[derive(Debug, Default)]
pub struct Methods(IndexSet<Method>);

impl TryFrom<Self> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> Result<Self, Self::Error> {
        Ok(Methods(indexset! { method }))
    }
}

impl<M> TryFrom<Vec<M>> for Methods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> Result<Self, Self::Error> {
        let methods = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<Result<_, _>>()
            .map_err(Into::into)?;
        Ok(Methods(methods))
    }
}

impl<'a> TryFrom<&'a str> for Methods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> Result<Self, Self::Error> {
        let methods = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(Methods(methods))
    }
}

pub trait PathExtractor: Send + Sync + 'static {
    type Output: Tuple;

    fn extract(&self, params: &Params<'_>) -> crate::Result<Self::Output>;
}

impl PathExtractor for () {
    type Output = ();

    #[inline]
    fn extract(&self, _: &Params<'_>) -> crate::Result<Self::Output> {
        Ok(())
    }
}

impl<E1, E2> PathExtractor for Chain<E1, E2>
where
    E1: PathExtractor,
    E2: PathExtractor,
    E1::Output: Combine<E2::Output>,
{
    type Output = <E1::Output as Combine<E2::Output>>::Out;

    #[inline]
    fn extract(&self, params: &Params<'_>) -> crate::Result<Self::Output> {
        let x = self.left.extract(params)?;
        let y = self.right.extract(params)?;
        Ok(x.combine(y))
    }
}

#[derive(Debug, Default)]
pub struct PathBuilder<E: PathExtractor = ()> {
    extractor: E,
    segments: Vec<String>,
}

impl<E: PathExtractor> PathBuilder<E> {
    pub fn segment(mut self, s: impl AsRef<str>) -> Self {
        self.segments.push(s.as_ref().into());
        self
    }

    pub fn param<T>(
        self,
        name: impl AsRef<str>,
    ) -> PathBuilder<impl PathExtractor<Output = <E::Output as Combine<(T,)>>::Out>>
    where
        T: FromParam,
        E::Output: Combine<(T,)>,
    {
        #[allow(missing_debug_implementations)]
        struct ExtractParam<T>(String, PhantomData<fn() -> T>);

        impl<T> PathExtractor for ExtractParam<T>
        where
            T: FromParam,
        {
            type Output = (T,);

            fn extract(&self, params: &Params<'_>) -> crate::Result<Self::Output> {
                let s = params
                    .name(&self.0)
                    .ok_or_else(|| crate::error::internal_server_error("unknown parameter name"))?;
                T::from_param(EncodedStr::new(s))
                    .map(|x| (x,))
                    .map_err(Into::into)
            }
        }

        PathBuilder {
            segments: {
                let mut segments = self.segments;
                segments.push(format!(":{}", name.as_ref()));
                segments
            },
            extractor: Chain::new(
                self.extractor,
                ExtractParam(name.as_ref().into(), PhantomData),
            ),
        }
    }

    pub fn catch_all<T>(
        self,
        name: impl AsRef<str>,
    ) -> Builder<impl Extractor<Output = <E::Output as Combine<(T,)>>::Out, Error = crate::Error>, ()>
    where
        T: FromParam,
        E::Output: Combine<(T,)>,
    {
        #[allow(missing_debug_implementations)]
        struct ExtractCatchAll<T>(PhantomData<fn() -> T>);

        impl<T> PathExtractor for ExtractCatchAll<T>
        where
            T: FromParam,
        {
            type Output = (T,);

            fn extract(&self, params: &Params<'_>) -> crate::Result<Self::Output> {
                let s = params.get_wildcard().ok_or_else(|| {
                    crate::error::internal_server_error("missing wildcard_parameter")
                })?;
                T::from_param(EncodedStr::new(s))
                    .map(|x| (x,))
                    .map_err(Into::into)
            }
        }

        (PathBuilder {
            segments: {
                let mut segments = self.segments;
                segments.push(format!("*{}", name.as_ref()));
                segments
            },
            extractor: Chain::new(self.extractor, ExtractCatchAll(PhantomData)),
        }).finalize(false)
    }

    pub fn end(self) -> Builder<impl Extractor<Output = E::Output, Error = crate::Error>, ()> {
        self.finalize(false)
    }

    pub fn slash(self) -> Builder<impl Extractor<Output = E::Output, Error = crate::Error>, ()> {
        self.finalize(true)
    }

    fn finalize(
        self,
        trailing_slash: bool,
    ) -> Builder<impl Extractor<Output = E::Output, Error = crate::Error>, ()> {
        #[derive(Debug)]
        struct ExtractParams<E>(E);

        impl<E> Extractor for ExtractParams<E>
        where
            E: PathExtractor,
        {
            type Output = E::Output;
            type Error = crate::Error;
            type Future = crate::common::NeverFuture<Self::Output, Self::Error>;

            #[inline]
            fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
                match input.params {
                    Some(params) => MaybeFuture::Ready(self.0.extract(&params)),
                    None => MaybeFuture::err(crate::error::internal_server_error("missing params")),
                }
            }
        }

        let uri = if trailing_slash {
            self.segments
                .into_iter()
                .fold(String::from("/"), |mut acc, s| {
                    acc.push_str(&*s);
                    acc.push('/');
                    acc
                }).parse()
                .expect("this is a bug.")
        } else {
            self.segments
                .into_iter()
                .enumerate()
                .fold(String::from("/"), |mut acc, (i, s)| {
                    if i > 0 {
                        acc.push('/');
                    }
                    acc.push_str(&*s);
                    acc
                }).parse()
                .expect("this is a bug.")
        };
        eprintln!("[dbg] uri = {:?}", uri);

        Builder::new(ExtractParams(self.extractor), (), uri)
    }
}

pub mod path {
    use super::{Builder, PathBuilder, Uri};

    pub fn root() -> Builder<(), ()> {
        Builder::new((), (), Uri::root())
    }

    pub fn asterisk() -> Builder<(), ()> {
        Builder::new((), (), Uri::asterisk())
    }

    pub fn builder() -> PathBuilder<()> {
        PathBuilder::default()
    }
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug, Default)]
pub struct Builder<E: Extractor = (), M = ()> {
    extractor: E,
    modifier: M,
    uri: Uri,
    methods: Methods,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E, M> Builder<E, M>
where
    E: Extractor,
{
    fn new(extractor: E, modifier: M, uri: Uri) -> Self {
        Builder {
            extractor,
            modifier,
            uri,
            methods: Methods(IndexSet::new()),
        }
    }

    /// Sets the HTTP methods that this route accepts.
    pub fn methods<M2>(self, methods: M2) -> super::Result<Self>
    where
        Methods: TryFrom<M2>,
    {
        Ok(Builder {
            methods: Methods::try_from(methods).map_err(Into::into)?,
            ..self
        })
    }

    /// Appends an `Extractor` to this builder.
    pub fn extract<U>(
        self,
        other: U,
    ) -> Builder<
        impl Extractor<Output = <E::Output as Combine<U::Output>>::Out, Error = crate::Error>,
        M,
    >
    where
        U: Extractor,
        E::Output: Combine<U::Output> + Send + 'static,
        U::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            modifier: self.modifier,
            uri: self.uri,
            methods: self.methods,
        }
    }

    /// Appends a `Modifier` to this builder.
    pub fn modify<M2>(self, modifier: M2) -> Builder<E, Chain<M, M2>> {
        Builder {
            extractor: self.extractor,
            modifier: Chain::new(self.modifier, modifier),
            uri: self.uri,
            methods: self.methods,
        }
    }

    pub fn finish<F>(self, make_handler: F) -> Route<F::Handler, M>
    where
        F: MakeHandler<E>,
    {
        Route {
            uri: self.uri,
            methods: self.methods,
            handler: make_handler.make_handler(self.extractor),
            modifier: self.modifier,
        }
    }
}

impl<E, M> Builder<E, M>
where
    E: Extractor,
{
    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value of `Responder`.
    pub fn reply<F>(self, f: F) -> Route<impl Handler<Output = F::Out, Error = crate::Error>, M>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
        F::Out: Responder,
    {
        #[allow(missing_debug_implementations)]
        struct ReplyHandlerFuture<Fut, F> {
            future: Fut,
            f: F,
        }

        impl<Fut, F> Future for ReplyHandlerFuture<Fut, F>
        where
            Fut: Future,
            Fut::Item: Tuple,
            Fut::Error: Into<crate::Error>,
            F: Func<Fut::Item>,
            F::Out: Responder,
        {
            type Item = F::Out;
            type Error = crate::Error;

            #[inline]
            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                let args = futures::try_ready!(self.future.poll().map_err(Into::into));
                Ok(self.f.call(args).into())
            }
        }

        #[allow(missing_debug_implementations)]
        struct ReplyHandler<E, F> {
            extractor: E,
            f: F,
        }

        impl<E, F> Handler for ReplyHandler<E, F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            type Output = F::Out;
            type Error = crate::Error;
            type Future = ReplyHandlerFuture<E::Future, F>;

            fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
                match self.extractor.extract(input) {
                    MaybeFuture::Ready(result) => {
                        MaybeFuture::Ready(result.map(|args| self.f.call(args)).map_err(Into::into))
                    }
                    MaybeFuture::Future(future) => MaybeFuture::Future(ReplyHandlerFuture {
                        future,
                        f: self.f.clone(),
                    }),
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct Reply<F>(F);

        impl<F, E> MakeHandler<E> for Reply<F>
        where
            E: Extractor,
            F: Func<E::Output> + Clone + Send + Sync + 'static,
            F::Out: Responder,
        {
            type Output = F::Out;
            type Error = crate::Error;
            type Handler = ReplyHandler<E, F>;

            fn make_handler(self, extractor: E) -> Self::Handler {
                ReplyHandler {
                    extractor,
                    f: self.0,
                }
            }
        }

        self.finish(Reply(f))
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The result of provided function is returned by `Future`.
    pub fn call<F, R>(self, f: F) -> Route<impl Handler<Output = R::Item, Error = crate::Error>, M>
    where
        F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture<Error = crate::Error>,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        #[allow(missing_debug_implementations)]
        struct CallHandlerFuture<Fut, F>
        where
            Fut: Future,
            Fut::Item: Tuple,
            Fut::Error: Into<crate::Error>,
            F: Func<Fut::Item>,
            F::Out: IntoFuture<Error = crate::Error>,
            <F::Out as IntoFuture>::Item: Responder,
        {
            state: State<Fut, <F::Out as IntoFuture>::Future, F>,
        }

        enum State<F1, F2, F> {
            First(F1, F),
            Second(F2),
        }

        impl<Fut, F> Future for CallHandlerFuture<Fut, F>
        where
            Fut: Future,
            Fut::Item: Tuple,
            Fut::Error: Into<crate::Error>,
            F: Func<Fut::Item>,
            F::Out: IntoFuture<Error = crate::Error>,
            <F::Out as IntoFuture>::Item: Responder,
        {
            type Item = <F::Out as IntoFuture>::Item;
            type Error = crate::Error;

            #[inline]
            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                loop {
                    self.state = match self.state {
                        State::First(ref mut f1, ref f) => {
                            let args = futures::try_ready!(f1.poll().map_err(Into::into));
                            State::Second(f.call(args).into_future())
                        }
                        State::Second(ref mut f2) => return f2.poll(),
                    }
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct CallHandler<E, F> {
            extractor: E,
            f: F,
        }

        impl<E, F, R> Handler for CallHandler<E, F>
        where
            E: Extractor,
            F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
            R: IntoFuture<Error = crate::Error>,
            R::Future: Send + 'static,
            R::Item: Responder,
        {
            type Output = R::Item;
            type Error = crate::Error;
            type Future = CallHandlerFuture<E::Future, F>;

            fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
                match self.extractor.extract(input) {
                    MaybeFuture::Ready(Ok(args)) => MaybeFuture::Future(CallHandlerFuture {
                        state: State::Second(self.f.call(args).into_future()),
                    }),
                    MaybeFuture::Ready(Err(err)) => MaybeFuture::err(err.into()),
                    MaybeFuture::Future(future) => MaybeFuture::Future(CallHandlerFuture {
                        state: State::First(future, self.f.clone()),
                    }),
                }
            }
        }

        #[allow(missing_debug_implementations)]
        struct Call<F>(F);

        impl<F, E, R> MakeHandler<E> for Call<F>
        where
            E: Extractor,
            F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
            R: IntoFuture<Error = crate::Error>,
            R::Future: Send + 'static,
            R::Item: Responder,
        {
            type Output = R::Item;
            type Error = crate::Error;
            type Handler = CallHandler<E, F>;

            fn make_handler(self, extractor: E) -> Self::Handler {
                CallHandler {
                    extractor,
                    f: self.0,
                }
            }
        }

        self.finish(Call(f))
    }
}

impl<E, M> Builder<E, M>
where
    E: Extractor<Output = ()>,
{
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> Route<H, M>
    where
        H: Handler,
    {
        #[allow(missing_debug_implementations)]
        struct Raw<H>(H);

        impl<H, E> MakeHandler<E> for Raw<H>
        where
            E: Extractor<Output = ()>,
            H: Handler,
        {
            type Output = H::Output;
            type Error = H::Error;
            type Handler = H;

            #[inline]
            fn make_handler(self, _: E) -> Self::Handler {
                self.0
            }
        }

        self.finish(Raw(handler))
    }

    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<T>(self, output: T) -> Route<impl Handler<Output = T, Error = crate::Error>, M>
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
    ) -> Route<impl Handler<Output = Redirect, Error = crate::Error>, M> {
        self.say(Redirect::new(status, location))
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> Route<impl Handler<Output = NamedFile, Error = crate::Error>, M> {
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

#[derive(Debug)]
pub struct Route<H, M> {
    methods: Methods,
    uri: Uri,
    handler: H,
    modifier: M,
}

impl<H, M1, M2> Scope<M1> for Route<H, M2>
where
    H: Handler,
    M2: Modifier<H>,
    M1: Modifier<M2::Out>,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M1>) -> Result<(), Self::Error> {
        cx.add_endpoint(self.uri, self.methods.0, self.modifier.modify(self.handler))
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
