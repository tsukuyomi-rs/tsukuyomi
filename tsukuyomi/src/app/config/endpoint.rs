use {
    crate::{
        core::{Chain, TryInto},
        endpoint::Endpoint,
        error::Error,
        extractor::Extractor,
        generic::{Combine, Func},
        handler::AllowedMethods,
    },
    futures01::IntoFuture,
    http::Method,
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
    E: Extractor,
{
    /// Appends a supplemental `Extractor` to this route.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output>,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            allowed_methods: self.allowed_methods,
        }
    }

    /// Creates an endpoint that replies its result immediately.
    pub fn call<T, F>(
        self,
        f: F,
    ) -> impl Endpoint<
        T,
        Output = F::Out,
        Action = self::call::CallAction<E, F>, // private
    >
    where
        T: Combine<E::Output>,
        F: Func<<T as Combine<E::Output>>::Out> + Clone,
    {
        let apply_fn = {
            let allowed_methods = self.allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);
            move |method: &http::Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }
                Some(self::call::CallAction {
                    extractor: extractor.clone(),
                    f: f.clone(),
                })
            }
        };
        crate::endpoint::endpoint(apply_fn, self.allowed_methods)
    }

    /// Creates an `Endpoint` that replies its result as a `Future`.
    pub fn call_async<T, F, R>(
        self,
        f: F,
    ) -> impl Endpoint<
        T,
        Output = R::Item,
        Action = self::call_async::CallAsyncAction<E, F>, // private
    >
    where
        T: Combine<E::Output>,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone,
        R: IntoFuture,
        R::Error: Into<Error>,
    {
        let apply_fn = {
            let allowed_methods = self.allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);
            move |method: &http::Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }

                Some(self::call_async::CallAsyncAction {
                    extractor: extractor.clone(),
                    f: f.clone(),
                })
            }
        };
        crate::endpoint::endpoint(apply_fn, self.allowed_methods)
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()>,
{
    /// Creates an `Endpoint` that replies the specified value.
    pub fn reply<R>(
        self,
        output: R,
    ) -> impl Endpoint<
        (), //
        Output = R,
        Action = self::call::CallAction<E, impl Func<(), Out = R> + Clone>, // private
    >
    where
        R: Clone,
    {
        self.call(move || output.clone())
    }
}

mod call {
    use {
        crate::{
            endpoint::EndpointAction,
            error::Error,
            extractor::Extractor,
            generic::{Combine, Func, Tuple},
            input::Input,
        },
        futures01::{Async, Future, Poll},
        std::sync::Arc,
    };

    #[derive(Debug)]
    pub struct CallAction<E, F> {
        pub(super) extractor: Arc<E>,
        pub(super) f: F,
    }

    impl<E, F, T> EndpointAction<T> for CallAction<E, F>
    where
        E: Extractor,
        F: Func<<T as Combine<E::Output>>::Out>,
        T: Combine<E::Output>,
    {
        type Output = F::Out;
        type Error = E::Error;
        type Future = CallFuture<E::Future, F, T>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            CallFuture {
                future: self.extractor.extract(input),
                f: self.f,
                args: Some(args),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct CallFuture<Fut, F, T> {
        future: Fut,
        f: F,
        args: Option<T>,
    }

    impl<Fut, F, T> Future for CallFuture<Fut, F, T>
    where
        Fut: Future,
        Fut::Item: Tuple,
        Fut::Error: Into<Error>,
        F: Func<<T as Combine<Fut::Item>>::Out>,
        T: Combine<Fut::Item>,
    {
        type Item = F::Out;
        type Error = Fut::Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let args2 = futures01::try_ready!(self.future.poll());
            let args = self
                .args
                .take()
                .expect("the future has already been polled.");
            Ok(Async::Ready(self.f.call(args.combine(args2))))
        }
    }
}

mod call_async {
    use {
        crate::{
            endpoint::EndpointAction,
            error::Error,
            extractor::Extractor,
            generic::{Combine, Func, Tuple},
            input::Input,
        },
        futures01::{Future, IntoFuture, Poll},
        std::sync::Arc,
    };

    #[allow(missing_debug_implementations)]
    pub struct CallAsyncAction<E, F> {
        pub(super) extractor: Arc<E>,
        pub(super) f: F,
    }

    impl<E, F, T, R> EndpointAction<T> for CallAsyncAction<E, F>
    where
        E: Extractor,
        F: Func<<T as Combine<E::Output>>::Out, Out = R>,
        T: Combine<E::Output>,
        R: IntoFuture,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = Error;
        type Future = CallAsyncFuture<E::Future, F, R, T>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            CallAsyncFuture {
                state: State::First(self.extractor.extract(input)),
                f: self.f,
                args: Some(args),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    enum State<Fut1, Fut2> {
        First(Fut1),
        Second(Fut2),
    }

    #[allow(missing_debug_implementations)]
    pub struct CallAsyncFuture<Fut, F, R: IntoFuture, T> {
        state: State<Fut, R::Future>,
        f: F,
        args: Option<T>,
    }

    impl<Fut, F, R, T> Future for CallAsyncFuture<Fut, F, R, T>
    where
        Fut: Future,
        Fut::Item: Tuple,
        Fut::Error: Into<Error>,
        F: Func<<T as Combine<Fut::Item>>::Out, Out = R>,
        R: IntoFuture,
        R::Error: Into<Error>,
        T: Combine<Fut::Item>,
    {
        type Item = R::Item;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            loop {
                self.state = match self.state {
                    State::First(ref mut future) => {
                        let args2 = futures01::try_ready!(future.poll().map_err(Into::into));
                        let args = self
                            .args
                            .take()
                            .expect("the future has already been polled.");
                        State::Second(self.f.call(args.combine(args2)).into_future())
                    }
                    State::Second(ref mut future) => return future.poll().map_err(Into::into),
                };
            }
        }
    }
}
