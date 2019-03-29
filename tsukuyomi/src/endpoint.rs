//! Definition of `Endpoint`.

use {
    crate::{
        error::Error,
        future::TryFuture,
        generic::{Func, Tuple},
    },
    futures01::IntoFuture,
    std::marker::PhantomData,
};

/// A trait representing the process to be performed when a route matches.
pub trait Endpoint<T> {
    type Output;
    type Error: Into<Error>;
    type Future: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Maps the provided arguments into a `TryFuture`.
    fn apply(&self, args: T) -> Self::Future;
}

/// A function to create an `Endpoint` from the specified components.
pub fn endpoint<T, R>(
    apply: impl Fn(T) -> R,
) -> impl Endpoint<T, Output = R::Ok, Error = R::Error, Future = R>
where
    R: TryFuture,
{
    #[allow(missing_debug_implementations)]
    struct ApplyFn<F> {
        apply: F,
    }

    impl<F, T, R> Endpoint<T> for ApplyFn<F>
    where
        F: Fn(T) -> R,
        R: TryFuture,
    {
        type Output = R::Ok;
        type Error = R::Error;
        type Future = R;

        #[inline]
        fn apply(&self, args: T) -> Self::Future {
            (self.apply)(args)
        }
    }

    ApplyFn { apply }
}

impl<E, T> Endpoint<T> for std::rc::Rc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn apply(&self, args: T) -> Self::Future {
        (**self).apply(args)
    }
}

impl<E, T> Endpoint<T> for std::sync::Arc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn apply(&self, args: T) -> Self::Future {
        (**self).apply(args)
    }
}

/// A shortcut to `endpoint::any().call(f)`
#[inline]
pub fn call<F, T>(f: F) -> Call<F, T>
where
    F: Func<T>,
    T: Tuple,
{
    Call {
        f,
        _marker: PhantomData,
    }
}

#[allow(missing_debug_implementations)]
pub struct Call<F, T>
where
    F: Func<T>,
    T: Tuple,
{
    f: F,
    _marker: PhantomData<fn(T)>,
}

mod call {
    use {
        super::{Call, Endpoint},
        crate::{
            future::{Async, Poll, TryFuture},
            generic::{Func, Tuple},
            input::Input,
            util::Never,
        },
    };

    impl<F, T> Endpoint<T> for Call<F, T>
    where
        F: Func<T>,
        T: Tuple,
    {
        type Output = F::Out;
        type Error = Never;
        type Future = CallFuture<F::Out>;

        fn apply(&self, args: T) -> Self::Future {
            CallFuture(Some(self.f.call(args)))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct CallFuture<R>(Option<R>);

    impl<R> TryFuture for CallFuture<R> {
        type Ok = R;
        type Error = Never;

        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            Ok(Async::Ready(
                self.0.take().expect("the future has already been polled."),
            ))
        }
    }
}

/// A shortcut to `endpoint::any().call_async(f)`.
pub fn call_async<F, T, R>(f: F) -> CallAsync<F, T, R>
where
    F: Func<T, Out = R>,
    T: Tuple,
    R: IntoFuture,
    R::Error: Into<Error>,
{
    CallAsync {
        f,
        _marker: PhantomData,
    }
}

#[allow(missing_debug_implementations)]
pub struct CallAsync<F, T, R>
where
    F: Func<T, Out = R>,
    T: Tuple,
    R: IntoFuture,
    R::Error: Into<Error>,
{
    f: F,
    _marker: PhantomData<fn(T) -> R>,
}

mod call_async {
    use {
        super::{CallAsync, Endpoint},
        crate::{
            error::Error,
            generic::{Func, Tuple},
        },
        futures01::IntoFuture,
    };

    impl<F, T, R> Endpoint<T> for CallAsync<F, T, R>
    where
        F: Func<T, Out = R> + Clone,
        T: Tuple,
        R: IntoFuture,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = crate::future::Compat01<R::Future>;

        fn apply(&self, args: T) -> Self::Future {
            crate::future::Compat01::from(self.f.call(args).into_future())
        }
    }
}
