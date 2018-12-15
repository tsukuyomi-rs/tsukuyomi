use {
    crate::{
        core::{Chain, TryInto},
        error::Error,
        extractor::Extractor,
        generic::{Combine, Func, Tuple},
        handler::AllowedMethods,
    },
    futures01::IntoFuture,
    http::Method,
    std::marker::PhantomData,
};

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
    E::Error: Send + 'static,
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

    /// Creates an endpoint that replies its result immediately.
    pub fn call<T, F>(self, f: F) -> self::call::CallEndpoint<T, E, F>
    where
        T: Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: Send + 'static,
    {
        self::call::CallEndpoint {
            allowed_methods: self.allowed_methods,
            extractor: std::sync::Arc::new(self.extractor),
            f,
            _marker: PhantomData,
        }
    }

    /// Creates an `Endpoint` that replies its result as a `Future`.
    pub fn call_async<T, F, R>(self, f: F) -> self::call_async::CallAsyncEndpoint<T, E, F, R>
    where
        E::Output: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone + Send + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        self::call_async::CallAsyncEndpoint {
            allowed_methods: self.allowed_methods,
            extractor: std::sync::Arc::new(self.extractor),
            f,
            _marker: PhantomData,
        }
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()> + Send + Sync + 'static,
    E::Error: Send + 'static,
{
    /// Creates an `Endpoint` that replies the specified value.
    pub fn reply<R>(
        self,
        output: R,
    ) -> self::call::CallEndpoint<(), E, impl Func<(), Out = R> + Clone>
    where
        R: Clone + Send + 'static,
    {
        self.call(move || output.clone())
    }
}

mod call {
    use {
        crate::{
            endpoint::{Endpoint, EndpointAction},
            extractor::Extractor,
            generic::{Combine, Func},
            handler::AllowedMethods,
            input::Input,
        },
        futures01::Future,
        std::{marker::PhantomData, sync::Arc},
    };

    #[derive(Debug)]
    pub struct CallEndpoint<T, E, F>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor + Send + Sync + 'static,
        E::Error: Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: Send + 'static,
    {
        pub(super) extractor: Arc<E>,
        pub(super) f: F,
        pub(super) allowed_methods: Option<AllowedMethods>,
        pub(super) _marker: PhantomData<fn(T)>,
    }

    impl<T, E, F> Endpoint<T> for CallEndpoint<T, E, F>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor + Send + Sync + 'static,
        E::Error: Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: Send + 'static,
    {
        type Output = F::Out;
        type Action = CallAction<T, E, F>;

        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        fn apply(&self, method: &http::Method) -> Option<Self::Action> {
            if self
                .allowed_methods
                .as_ref()
                .map_or(false, |methods| !methods.contains(method))
            {
                return None;
            }

            let f = self.f.clone();
            let extractor = self.extractor.clone();

            Some(CallAction {
                extractor,
                f,
                _marker: PhantomData,
            })
        }
    }

    #[derive(Debug)]
    pub struct CallAction<T, E, F>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor,
        E::Error: Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Send + 'static,
        F::Out: Send + 'static,
    {
        extractor: Arc<E>,
        f: F,
        _marker: PhantomData<fn(T)>,
    }

    impl<T, E, F> EndpointAction<T> for CallAction<T, E, F>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor,
        E::Error: Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Send + 'static,
        F::Out: Send + 'static,
    {
        type Output = F::Out;
        type Error = E::Error;
        type Future = Box<dyn Future<Item = Self::Output, Error = Self::Error> + Send + 'static>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            let Self { extractor, f, .. } = self;
            Box::new(
                extractor
                    .extract(input)
                    .then(move |result| result.map(|args2| f.call(args.combine(args2)))),
            )
        }
    }
}

mod call_async {
    use {
        crate::{
            endpoint::{Endpoint, EndpointAction},
            error::Error,
            extractor::Extractor,
            generic::{Combine, Func},
            handler::AllowedMethods,
            input::Input,
        },
        futures01::{Future, IntoFuture},
        std::{marker::PhantomData, sync::Arc},
    };

    #[derive(Debug)]
    pub struct CallAsyncEndpoint<T, E, F, R> {
        pub(super) allowed_methods: Option<AllowedMethods>,
        pub(super) extractor: Arc<E>,
        pub(super) f: F,
        pub(super) _marker: PhantomData<fn(T) -> R>,
    }

    impl<T, E, F, R> Endpoint<T> for CallAsyncEndpoint<T, E, F, R>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor + Send + Sync + 'static,
        E::Output: Send + 'static,
        E::Error: 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone + Send + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Action = CallAsyncAction<T, E, F, R>;

        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        fn apply(&self, method: &http::Method) -> Option<Self::Action> {
            if self
                .allowed_methods
                .as_ref()
                .map_or(false, |methods| !methods.contains(method))
            {
                return None;
            }

            let f = self.f.clone();
            let extractor = self.extractor.clone();

            Some(CallAsyncAction {
                extractor,
                f,
                _marker: PhantomData,
            })
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct CallAsyncAction<T, E, F, R> {
        extractor: Arc<E>,
        f: F,
        _marker: PhantomData<fn(T) -> R>,
    }

    impl<T, E, F, R> EndpointAction<T> for CallAsyncAction<T, E, F, R>
    where
        T: Combine<E::Output> + Send + 'static,
        E: Extractor,
        E::Output: Send + 'static,
        E::Error: 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Send + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = Error;
        type Future = Box<dyn Future<Item = Self::Output, Error = Self::Error> + Send + 'static>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            let Self { extractor, f, .. } = self;
            Box::new(
                extractor
                    .extract(input)
                    .map_err(Into::into)
                    .and_then(move |args2| {
                        f.call(args.combine(args2))
                            .into_future()
                            .map_err(Into::into)
                    }),
            )
        }
    }
}
