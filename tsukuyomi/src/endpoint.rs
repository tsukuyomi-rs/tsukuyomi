use {
    crate::{
        core::{Chain, TryInto},
        error::Error,
        extractor::Extractor,
        fs::NamedFile,
        generic::{Combine, Func, Tuple},
        handler::AllowedMethods,
        input::Input,
    },
    futures01::{Future, IntoFuture},
    http::Method,
    std::path::Path,
};

pub trait EndpointAction<T> {
    type Output;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    fn call(self, input: &mut Input<'_>, args: T) -> Self::Future;
}

pub fn action<T, R>(
    f: impl FnOnce(&mut Input<'_>, T) -> R,
) -> impl EndpointAction<T, Output = R::Item, Future = R::Future>
where
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Error: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct EndpointActionFn<F>(F);

    impl<F, T, R> EndpointAction<T> for EndpointActionFn<F>
    where
        F: FnOnce(&mut Input<'_>, T) -> R,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            (self.0)(input, args).into_future()
        }
    }

    EndpointActionFn(f)
}

pub trait Endpoint<T> {
    type Output;
    type Action: EndpointAction<T, Output = Self::Output> + Send + 'static;

    /// Returns a list of HTTP methods that the returned endpoint accepts.
    ///
    /// If it returns a `None`, it means that the endpoint accepts *all* methods.
    fn allowed_methods(&self) -> Option<AllowedMethods>;

    fn apply(&self, method: &Method) -> Option<Self::Action>;
}

pub fn endpoint<T, A>(
    apply: impl Fn(&Method) -> Option<A>,
    allowed_methods: Option<AllowedMethods>,
) -> impl Endpoint<T, Output = A::Output, Action = A>
where
    A: EndpointAction<T> + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct ApplyFn<F> {
        apply: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T, A> Endpoint<T> for ApplyFn<F>
    where
        F: Fn(&Method) -> Option<A>,
        A: EndpointAction<T> + Send + 'static,
    {
        type Output = A::Output;
        type Action = A;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        #[inline]
        fn apply(&self, method: &Method) -> Option<Self::Action> {
            (self.apply)(method)
        }
    }

    ApplyFn {
        apply,
        allowed_methods,
    }
}

impl<E, T> Endpoint<T> for std::rc::Rc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Action = E::Action;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn apply(&self, method: &Method) -> Option<Self::Action> {
        (**self).apply(method)
    }
}

impl<E, T> Endpoint<T> for std::sync::Arc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Action = E::Action;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn apply(&self, method: &Method) -> Option<Self::Action> {
        (**self).apply(method)
    }
}

mod impl_chain {
    use {
        super::{Endpoint, EndpointAction},
        crate::{core::Chain, error::Error, handler::AllowedMethods, input::Input},
        either::Either,
        futures01::{Future, Poll},
        http::Method,
    };

    impl<L, R, T> Endpoint<T> for Chain<L, R>
    where
        L: Endpoint<T>,
        R: Endpoint<T>,
    {
        type Output = Either<L::Output, R::Output>;
        type Action = ChainAction<L::Action, R::Action>;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            let left = self.left.allowed_methods()?;
            let right = self.right.allowed_methods()?;
            Some(left.iter().chain(right.iter()).cloned().collect())
        }

        #[inline]
        fn apply(&self, method: &Method) -> Option<Self::Action> {
            self.left
                .apply(method)
                .map(ChainAction::Left)
                .or_else(|| self.right.apply(method).map(ChainAction::Right))
        }
    }

    #[derive(Debug)]
    pub enum ChainAction<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R, T> EndpointAction<T> for ChainAction<L, R>
    where
        L: EndpointAction<T>,
        R: EndpointAction<T>,
    {
        type Output = Either<L::Output, R::Output>;
        type Error = Error;
        type Future = ChainFuture<L::Future, R::Future>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            match self {
                ChainAction::Left(l) => ChainFuture::Left(l.call(input, args)),
                ChainAction::Right(r) => ChainFuture::Right(r.call(input, args)),
            }
        }
    }

    #[derive(Debug)]
    pub enum ChainFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> Future for ChainFuture<L, R>
    where
        L: Future,
        R: Future,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
    {
        type Item = Either<L::Item, R::Item>;
        type Error = Error;

        #[inline]
        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self {
                ChainFuture::Left(l) => l.poll().map(|x| x.map(Either::Left)).map_err(Into::into),
                ChainFuture::Right(r) => r.poll().map(|x| x.map(Either::Right)).map_err(Into::into),
            }
        }
    }
}

// ==== builder ====

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
    pub fn reply<T, F>(self, f: F) -> impl Endpoint<T, Output = F::Out>
    where
        E::Output: Send + 'static,
        E::Error: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: Send + 'static,
    {
        let allowed_methods = self.allowed_methods.clone();

        let apply = {
            let extractor = std::sync::Arc::new(self.extractor);
            let allowed_methods = allowed_methods.clone();
            move |method: &Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }
                let f = f.clone();
                let extractor = extractor.clone();
                Some(crate::endpoint::action(move |input, args: T| {
                    extractor
                        .extract(input)
                        .then(move |result| result.map(|args2| f.call(args.combine(args2))))
                }))
            }
        };

        self::endpoint(apply, allowed_methods)
    }

    /// Creates a `Dispatcher` with the specified function that returns its result as a `Future`.
    pub fn call<T, F, R>(self, f: F) -> impl Endpoint<T, Output = R::Item>
    where
        E::Output: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone + Send + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        let allowed_methods = self.allowed_methods.clone();

        let apply = {
            let allowed_methods = allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);

            move |method: &Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }

                let f = f.clone();
                let extractor = extractor.clone();

                Some(crate::endpoint::action(move |input, args: T| {
                    extractor
                        .extract(input)
                        .map_err(Into::into)
                        .and_then(move |args2| {
                            f.call(args.combine(args2))
                                .into_future()
                                .map_err(Into::into)
                        })
                }))
            }
        };

        self::endpoint(apply, allowed_methods)
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()> + Send + Sync + 'static,
    E::Error: Send + 'static,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<R>(self, output: R) -> impl Endpoint<(), Output = R>
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
    ) -> impl Endpoint<(), Output = NamedFile> {
        let path = crate::fs::ArcPath::from(path.as_ref().to_path_buf());

        self.call(move || match config {
            Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
            None => NamedFile::open(path.clone()),
        })
    }
}
