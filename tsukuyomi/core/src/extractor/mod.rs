//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod and;
mod fallible;
mod from_input;
mod generic;
mod map;
mod optional;
mod or;

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

pub use self::and::And;
pub use self::fallible::Fallible;
pub use self::from_input::{
    extension, local, method, state, uri, version, HasExtractor, LocalExtractor,
};
pub(crate) use self::generic::{Combine, Func, Tuple};
pub use self::map::Map;
pub use self::optional::Optional;
pub use self::or::Or;

// ==== impl ====

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use futures::future;
use futures::{Async, Future, IntoFuture, Poll};

use crate::error::{Error, Never};
use crate::input::Input;

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct Placeholder<T, E>(PhantomData<fn() -> (T, E)>);

impl<T, E> Future for Placeholder<T, E> {
    type Item = T;
    type Error = E;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unreachable!("The implementation of Extractor is wrong.")
    }
}

pub trait Extractor: Send + Sync + 'static {
    type Output: Tuple;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error>;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match (**self).extract(input)? {
            Extract::Ready(out) => Ok(Extract::Ready(out)),
            Extract::Incomplete(future) => Ok(Extract::Incomplete(future)),
        }
    }
}

impl<E> Extractor for Arc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match (**self).extract(input)? {
            Extract::Ready(out) => Ok(Extract::Ready(out)),
            Extract::Incomplete(future) => Ok(Extract::Incomplete(future)),
        }
    }
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Future = Placeholder<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        Ok(Extract::Ready(()))
    }
}

pub enum Extract<E: Extractor + ?Sized> {
    Ready(E::Output),
    Incomplete(E::Future),
}

impl<E> fmt::Debug for Extract<E>
where
    E: Extractor + ?Sized,
    E::Output: fmt::Debug,
    E::Future: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Extract::Ready(ref out) => f.debug_tuple("Ready").field(out).finish(),
            Extract::Incomplete(ref cx) => f.debug_tuple("Incomplete").field(cx).finish(),
        }
    }
}

// ==== ExtractorExt ====

#[inline]
pub(crate) fn assert_impl_extractor<E>(extractor: E) -> E
where
    E: Extractor,
{
    extractor
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ExtractorExt: Extractor + Sized {
    fn optional<T>(self) -> Optional<Self>
    where
        Self: Extractor<Output = (T,)>,
    {
        assert_impl_extractor(Optional(self))
    }

    fn fallible<T>(self) -> Fallible<Self>
    where
        Self: Extractor<Output = (T,)>,
    {
        assert_impl_extractor(Fallible(self))
    }

    fn and<E>(self, other: E) -> And<Self, E>
    where
        E: Extractor,
        Self::Output: Combine<E::Output> + Send + 'static,
        E::Output: Send + 'static,
    {
        assert_impl_extractor(And {
            left: self,
            right: other,
        })
    }

    fn or<E>(self, other: E) -> Or<Self, E>
    where
        E: Extractor<Output = Self::Output>,
    {
        assert_impl_extractor(Or {
            left: self,
            right: other,
        })
    }

    fn map<F, T, R>(self, f: F) -> Map<Self, F>
    where
        Self: Extractor<Output = (T,)>,
        F: Fn(T) -> R + Send + Sync + 'static,
    {
        assert_impl_extractor(Map {
            extractor: self,
            f: Arc::new(f),
        })
    }
}

impl<E: Extractor> ExtractorExt for E {}

pub fn validate<F, E>(f: F) -> impl Extractor<Output = ()>
where
    F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
    E: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Validate<F>(F);

    impl<F, E> Extractor for Validate<F>
    where
        F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
        E: Into<Error> + 'static,
    {
        type Output = ();
        type Error = E;
        type Future = self::Placeholder<Self::Output, Self::Error>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            (self.0)(input).map(Extract::Ready)
        }
    }

    assert_impl_extractor(Validate(f))
}

pub fn extractor<F, R>(f: F) -> impl Extractor<Output = (R::Item,)>
where
    F: Fn(&mut Input<'_>) -> R + Send + Sync + 'static,
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: 'static,
    R::Error: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct ExtractorFn<F>(F);

    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    impl<F, R> Extractor for ExtractorFn<F>
    where
        F: Fn(&mut Input<'_>) -> R + Send + Sync + 'static,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Item: 'static,
        R::Error: Into<Error> + 'static,
    {
        type Output = (R::Item,);
        type Error = R::Error;
        type Future = futures::future::Map<R::Future, fn(R::Item) -> (R::Item,)>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            Ok(Extract::Incomplete(
                (self.0)(input)
                    .into_future()
                    .map((|output| (output,)) as fn(R::Item) -> (R::Item,)),
            ))
        }
    }

    assert_impl_extractor(ExtractorFn(f))
}
