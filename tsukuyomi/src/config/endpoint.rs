use {
    crate::{
        endpoint::{ApplyContext, ApplyError, Endpoint},
        extractor::Extractor,
        generic::{Combine, Func},
        handler::AllowedMethods,
        util::{Chain, TryInto},
    },
    futures01::IntoFuture,
    http::Method,
};

pub fn any() -> Builder {
    Builder::allow_any()
}

pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> super::Result<Builder> {
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

    pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> super::Result<Self> {
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
            move |cx: &mut ApplyContext<'_, '_>| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(cx.method()))
                {
                    return Err(ApplyError::method_not_allowed());
                }
                Ok(self::call::CallAction {
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
        R::Error: Into<crate::error::Error>,
    {
        let apply_fn = {
            let allowed_methods = self.allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);
            move |cx: &mut ApplyContext<'_, '_>| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(cx.method()))
                {
                    return Err(ApplyError::method_not_allowed());
                }

                Ok(self::call_async::CallAsyncAction {
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
            extractor::Extractor,
            future::{Async, Poll, TryFuture},
            generic::{Combine, Func, Tuple},
            input::Input,
        },
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
        type Future = CallFuture<E::Extract, F, T>;

        fn invoke(self, args: T) -> Self::Future {
            CallFuture {
                extract: self.extractor.extract(),
                f: self.f,
                args: Some(args),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct CallFuture<E, F, T> {
        extract: E,
        f: F,
        args: Option<T>,
    }

    impl<E, F, T> TryFuture for CallFuture<E, F, T>
    where
        E: TryFuture,
        E::Ok: Tuple,
        F: Func<<T as Combine<E::Ok>>::Out>,
        T: Combine<E::Ok>,
    {
        type Ok = F::Out;
        type Error = E::Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let args2 = futures01::try_ready!(self.extract.poll_ready(input));
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
            future::{Poll, TryFuture},
            generic::{Combine, Func, Tuple},
            input::Input,
        },
        futures01::{Future, IntoFuture},
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
        type Future = CallAsyncFuture<E::Extract, F, R, T>;

        fn invoke(self, args: T) -> Self::Future {
            CallAsyncFuture {
                state: State::First(self.extractor.extract()),
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
    pub struct CallAsyncFuture<E, F, R: IntoFuture, T> {
        state: State<E, R::Future>,
        f: F,
        args: Option<T>,
    }

    impl<E, F, R, T> TryFuture for CallAsyncFuture<E, F, R, T>
    where
        E: TryFuture,
        E::Ok: Tuple,
        F: Func<<T as Combine<E::Ok>>::Out, Out = R>,
        R: IntoFuture,
        R::Error: Into<Error>,
        T: Combine<E::Ok>,
    {
        type Ok = R::Item;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                self.state = match self.state {
                    State::First(ref mut extract) => {
                        let args2 =
                            futures01::try_ready!(extract.poll_ready(input).map_err(Into::into));
                        let args = self
                            .args
                            .take()
                            .expect("the future has already been polled.");
                        State::Second(self.f.call(args.combine(args2)).into_future())
                    }
                    State::Second(ref mut action) => return action.poll().map_err(Into::into),
                };
            }
        }
    }
}
