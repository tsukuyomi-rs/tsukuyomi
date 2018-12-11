use {
    crate::{
        core::{Chain, TryInto},
        extractor::Extractor,
        fs::NamedFile,
        future::{Future, MaybeFuture},
        generic::{Combine, Func, Tuple},
        handler::AllowedMethods,
        input::Input,
    },
    either::Either,
    http::Method,
    std::path::Path,
};

pub trait Endpoint<T> {
    type Output;
    type Future: Future<Output = Self::Output> + Send + 'static;

    fn call(self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future>;
}

impl<L, R, T> Endpoint<T> for Either<L, R>
where
    L: Endpoint<T>,
    R: Endpoint<T>,
{
    type Output = Either<L::Output, R::Output>;
    type Future = Either<L::Future, R::Future>;

    fn call(self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
        match self {
            Either::Left(l) => match l.call(input, args) {
                MaybeFuture::Ready(result) => {
                    MaybeFuture::Ready(result.map(Either::Left).map_err(Into::into))
                }
                MaybeFuture::Future(future) => MaybeFuture::Future(Either::Left(future)),
            },
            Either::Right(r) => match r.call(input, args) {
                MaybeFuture::Ready(result) => {
                    MaybeFuture::Ready(result.map(Either::Right).map_err(Into::into))
                }
                MaybeFuture::Future(future) => MaybeFuture::Future(Either::Right(future)),
            },
        }
    }
}

pub fn endpoint<T, R>(
    f: impl FnOnce(&mut Input<'_>, T) -> MaybeFuture<R>,
) -> impl Endpoint<T, Output = R::Output, Future = R>
where
    R: Future + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct EndpointFn<F>(F);

    impl<F, T, R> Endpoint<T> for EndpointFn<F>
    where
        F: FnOnce(&mut Input<'_>, T) -> MaybeFuture<R>,
        R: Future + Send + 'static,
    {
        type Output = R::Output;
        type Future = R;

        fn call(self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
            (self.0)(input, args)
        }
    }

    EndpointFn(f)
}

pub trait Dispatcher<T> {
    type Output;
    type Endpoint: Endpoint<T, Output = Self::Output> + Send + 'static;

    /// Returns a list of HTTP methods that the returned endpoint accepts.
    ///
    /// If it returns a `None`, it means that the endpoint accepts *all* methods.
    fn allowed_methods(&self) -> Option<AllowedMethods>;

    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint>;
}

pub fn dispatcher<T, A>(
    dispatch: impl Fn(&mut Input<'_>) -> Option<A>,
    allowed_methods: Option<AllowedMethods>,
) -> impl Dispatcher<T, Output = A::Output, Endpoint = A>
where
    A: Endpoint<T> + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct DispatcherFn<F> {
        dispatch: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T, A> Dispatcher<T> for DispatcherFn<F>
    where
        F: Fn(&mut Input<'_>) -> Option<A>,
        A: Endpoint<T> + Send + 'static,
    {
        type Output = A::Output;
        type Endpoint = A;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        #[inline]
        fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
            (self.dispatch)(input)
        }
    }

    DispatcherFn {
        dispatch,
        allowed_methods,
    }
}

impl<E, T> Dispatcher<T> for std::rc::Rc<E>
where
    E: Dispatcher<T>,
{
    type Output = E::Output;
    type Endpoint = E::Endpoint;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        (**self).dispatch(input)
    }
}

impl<E, T> Dispatcher<T> for std::sync::Arc<E>
where
    E: Dispatcher<T>,
{
    type Output = E::Output;
    type Endpoint = E::Endpoint;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        (**self).dispatch(input)
    }
}

impl<L, R, T> Dispatcher<T> for Chain<L, R>
where
    L: Dispatcher<T>,
    R: Dispatcher<T>,
{
    type Output = Either<L::Output, R::Output>;
    type Endpoint = Either<L::Endpoint, R::Endpoint>;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        let left = self.left.allowed_methods()?;
        let right = self.right.allowed_methods()?;
        Some(left.iter().chain(right.iter()).cloned().collect())
    }

    #[inline]
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        self.left
            .dispatch(input)
            .map(Either::Left)
            .or_else(|| self.right.dispatch(input).map(Either::Right))
    }
}

pub fn any() -> Builder {
    Builder::allow_any()
}

pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> crate::app::Result<Builder> {
    Builder::allow_only(methods)
}

macro_rules! define_builder_with_allowing_sigle_method {
    ($(
        $(#[$m:meta])*
        $name:ident => $METHOD:ident,
    )*) => {$(
        $(#[$m])*
        pub fn $name() -> Builder {
            Builder {
                extractor: (),
                allowed_methods: Some(Method::$METHOD.into()),
            }
        }
    )*}
}

define_builder_with_allowing_sigle_method! {
    get => GET,
    post => POST,
    put => PUT,
    delete => DELETE,
    head => HEAD,
    options => OPTIONS,
    connect => CONNECT,
    patch => PATCH,
    trace => TRACE,
}

pub fn get_or_head() -> Builder {
    Builder::allow_only(vec![Method::GET, Method::HEAD]).expect("should be valid methods")
}

#[derive(Debug)]
pub struct Builder<E: Extractor = ()> {
    extractor: E,
    allowed_methods: Option<AllowedMethods>,
}

impl Builder {
    pub fn allow_any() -> Self {
        Self {
            extractor: (),
            allowed_methods: None,
        }
    }

    pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> crate::app::Result<Self> {
        Ok(Self {
            extractor: (),
            allowed_methods: Some(methods.try_into()?),
        })
    }
}

impl<E> Builder<E>
where
    E: Extractor + Send + Sync + 'static,
{
    /// Appends a supplemental `Extractor` to this route.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output> + Send + 'static,
        E2::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            allowed_methods: self.allowed_methods,
        }
    }

    /// Creates a `Dispatcher` with the specified function that returns its result immediately.
    pub fn reply<T, F>(self, f: F) -> impl Dispatcher<T, Output = F::Out>
    where
        E::Output: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: 'static,
    {
        let allowed_methods = self.allowed_methods.clone();

        let dispatch = {
            let extractor = std::sync::Arc::new(self.extractor);
            let allowed_methods = allowed_methods.clone();
            move |input: &mut crate::input::Input<'_>| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(input.request.method()))
                {
                    return None;
                }
                let f = f.clone();
                let extractor = extractor.clone();
                Some(crate::endpoint::endpoint(move |input, args: T| {
                    extractor
                        .extract(input)
                        .map_result(move |result| result.map(|args2| f.call(args.combine(args2))))
                }))
            }
        };

        self::dispatcher(dispatch, allowed_methods)
    }

    /// Creates a `Dispatcher` with the specified function that returns its result as a `Future`.
    pub fn call<T, F, R>(self, f: F) -> impl Dispatcher<T, Output = R::Output>
    where
        E::Output: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone + Send + 'static,
        R: Future + Send + 'static,
        F::Out: 'static,
    {
        let allowed_methods = self.allowed_methods.clone();

        let dispatch = {
            let allowed_methods = allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);

            move |input: &mut crate::input::Input<'_>| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(input.request.method()))
                {
                    return None;
                }

                let f = f.clone();
                let extractor = extractor.clone();

                #[allow(missing_debug_implementations)]
                enum State<F1, F2, T> {
                    First(F1, Option<T>),
                    Second(F2),
                }

                Some(crate::endpoint::endpoint(move |input, args: T| {
                    let mut state = match extractor.extract(input) {
                        MaybeFuture::Ready(Ok(args2)) => State::Second(f.call(args.combine(args2))),
                        MaybeFuture::Ready(Err(err)) => return MaybeFuture::err(err.into()),
                        MaybeFuture::Future(future) => State::First(future, Some((f, args))),
                    };

                    MaybeFuture::Future(crate::future::poll_fn(move |cx| loop {
                        state = match state {
                            State::First(ref mut future, ref mut x) => {
                                let args2 = futures01::try_ready!(future
                                    .poll_ready(cx)
                                    .map_err(Into::into));
                                let (f, args) = x.take().expect("the future has already polled.");
                                State::Second(f.call(args.combine(args2)))
                            }
                            State::Second(ref mut future) => {
                                return future.poll_ready(cx).map_err(Into::into)
                            }
                        };
                    }))
                }))
            }
        };

        self::dispatcher(dispatch, allowed_methods)
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()> + Send + Sync + 'static,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<R>(self, output: R) -> impl Dispatcher<(), Output = R>
    where
        R: Clone + Send + 'static,
    {
        self.reply(move || output.clone())
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> impl Dispatcher<(), Output = NamedFile> {
        let path = crate::fs::ArcPath::from(path.as_ref().to_path_buf());

        self.call(move || {
            crate::future::Compat01::from(match config {
                Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
                None => NamedFile::open(path.clone()),
            })
        })
    }
}
