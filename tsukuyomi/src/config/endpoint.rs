use {
    crate::{
        endpoint::{ApplyContext, ApplyError, Endpoint},
        error::Error,
        extractor::Extractor,
        generic::{Combine, Func},
        handler::metadata::AllowedMethods,
        util::{Chain, Never, TryInto},
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
                allowed_methods: Method::$METHOD.into(),
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

/// A builder of `Endpoint`.
#[derive(Debug)]
pub struct Builder<E: Extractor = ()> {
    extractor: E,
    allowed_methods: AllowedMethods,
}

impl Builder {
    /// Creates a `Builder` that accepts the all of HTTP methods.
    pub fn allow_any() -> Self {
        Self {
            extractor: (),
            allowed_methods: AllowedMethods::any(),
        }
    }

    /// Creates a `Builder` that accepts only the specified HTTP methods.
    pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> super::Result<Self> {
        Ok(Self {
            extractor: (),
            allowed_methods: methods.try_into().map_err(super::Error::custom)?,
        })
    }
}

impl<E> Builder<E>
where
    E: Extractor,
{
    /// Appends a supplemental `Extractor` to this endpoint.
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
        Error = E::Error,
        Future = self::call::CallFuture<E, F, T>, // private
    >
    where
        T: Combine<E::Output>,
        F: Func<<T as Combine<E::Output>>::Out> + Clone,
    {
        let apply_fn = {
            let allowed_methods = self.allowed_methods.clone();
            let extractor = self.extractor;
            move |args: T, cx: &mut ApplyContext<'_, '_>| {
                if !allowed_methods.contains(cx.method()) {
                    return Err((args, ApplyError::method_not_allowed()));
                }
                Ok(self::call::CallFuture {
                    extract: extractor.extract(),
                    f: f.clone(),
                    args: Some(args),
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
        Error = Error,
        Future = self::call_async::CallAsyncFuture<E, F, R, T>, // private
    >
    where
        T: Combine<E::Output>,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone,
        R: IntoFuture,
        R::Error: Into<Error>,
    {
        let apply_fn = {
            let allowed_methods = self.allowed_methods.clone();
            let extractor = self.extractor;
            move |args: T, cx: &mut ApplyContext<'_, '_>| {
                if !allowed_methods.contains(cx.method()) {
                    return Err((args, ApplyError::method_not_allowed()));
                }

                Ok(self::call_async::CallAsyncFuture {
                    state: self::call_async::State::First(extractor.extract()),
                    f: f.clone(),
                    args: Some(args),
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
        Error = E::Error,
        Future = self::call::CallFuture<E, impl Func<(), Out = R>, ()>, // private
    >
    where
        R: Clone,
    {
        self.call(move || output.clone())
    }
}

/// A shortcut to `endpoint::any().call(f)`
#[inline]
pub fn call<T, F>(
    f: F,
) -> impl Endpoint<
    T, //
    Output = F::Out,
    Error = Never,
    Future = self::call::CallFuture<(), F, T>, // private
>
where
    T: Combine<()>,
    F: Func<<T as Combine<()>>::Out> + Clone,
{
    any().call(f)
}

/// A shortcut to `endpoint::any().call_async(f)`.
pub fn call_async<T, F, R>(
    f: F,
) -> impl Endpoint<
    T,
    Output = R::Item,
    Error = Error,
    Future = self::call_async::CallAsyncFuture<(), F, R, T>, // private
>
where
    T: Combine<()>,
    F: Func<<T as Combine<()>>::Out, Out = R> + Clone,
    R: IntoFuture,
    R::Error: Into<Error>,
{
    any().call_async(f)
}

/// A shortcut to `endpoint::any().reply(output)`.
#[inline]
pub fn reply<R>(
    output: R,
) -> impl Endpoint<
    (), //
    Output = R,
    Error = Never,
    Future = self::call::CallFuture<(), impl Func<(), Out = R>, ()>,
>
where
    R: Clone,
{
    any().reply(output)
}

mod call {
    use crate::{
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        generic::{Combine, Func},
        input::Input,
    };

    #[allow(missing_debug_implementations)]
    pub struct CallFuture<E: Extractor, F, T> {
        pub(super) extract: E::Extract,
        pub(super) f: F,
        pub(super) args: Option<T>,
    }

    impl<E, F, T> TryFuture for CallFuture<E, F, T>
    where
        E: Extractor,
        F: Func<<T as Combine<E::Output>>::Out>,
        T: Combine<E::Output>,
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
            error::Error,
            extractor::Extractor,
            future::{Poll, TryFuture},
            generic::{Combine, Func},
            input::Input,
        },
        futures01::{Future, IntoFuture},
    };

    #[allow(missing_debug_implementations)]
    pub(super) enum State<Fut1, Fut2> {
        First(Fut1),
        Second(Fut2),
    }

    #[allow(missing_debug_implementations)]
    pub struct CallAsyncFuture<E: Extractor, F, R: IntoFuture, T> {
        pub(super) state: State<E::Extract, R::Future>,
        pub(super) f: F,
        pub(super) args: Option<T>,
    }

    impl<E, F, R, T> TryFuture for CallAsyncFuture<E, F, R, T>
    where
        E: Extractor,
        F: Func<<T as Combine<E::Output>>::Out, Out = R>,
        R: IntoFuture,
        R::Error: Into<Error>,
        T: Combine<E::Output>,
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
