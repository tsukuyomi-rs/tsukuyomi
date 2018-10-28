//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod from_input;

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

pub use self::from_input::{Extension, HasExtractor, LocalExtractor, State};

// ==== impl ====

use std::fmt;
use std::marker::PhantomData;

use either::Either;
use futures::future;
use futures::{Async, Future, Poll};

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

pub trait Extractor {
    type Output;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error>;
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

pub(crate) fn extract<E>(
    extractor: &E,
    input: &mut Input<'_>,
) -> impl Future<Item = E::Output, Error = E::Error>
where
    E: Extractor + ?Sized,
{
    use futures::future::{err, ok, Either};

    match extractor.extract(input) {
        Ok(Extract::Ready(out)) => Either::A(ok(out)),
        Ok(Extract::Incomplete(future)) => Either::B(future),
        Err(e) => Either::A(err(e)),
    }
}

// ==== ExtractorExt ====

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ExtractorExt: Extractor + Sized {
    fn optional(self) -> Optional<Self> {
        Optional(self)
    }

    fn fallible(self) -> Fallible<Self> {
        Fallible(self)
    }

    fn either_or<E>(self, other: E) -> EitherOr<Self, E>
    where
        E: Extractor,
    {
        EitherOr {
            left: self,
            right: other,
        }
    }
}

impl<E: Extractor> ExtractorExt for E {}

#[derive(Debug)]
pub struct Optional<E>(E);

impl<E> Extractor for Optional<E>
where
    E: Extractor,
{
    type Output = Option<E::Output>;
    type Error = Never;
    type Future = OptionalFuture<E::Future>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.0.extract(input) {
            Ok(Extract::Ready(out)) => Ok(Extract::Ready(Some(out))),
            Ok(Extract::Incomplete(future)) => Ok(Extract::Incomplete(OptionalFuture(future))),
            Err(..) => Ok(Extract::Ready(None)),
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct OptionalFuture<F>(F);

impl<F> Future for OptionalFuture<F>
where
    F: Future,
{
    type Item = Option<F::Item>;
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0.poll() {
            Ok(Async::Ready(out)) => Ok(Async::Ready(Some(out))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(..) => Ok(Async::Ready(None)),
        }
    }
}

#[derive(Debug)]
pub struct Fallible<E>(E);

impl<E> Extractor for Fallible<E>
where
    E: Extractor,
{
    type Output = Result<E::Output, E::Error>;
    type Error = Never;
    type Future = FallibleFuture<E::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.0.extract(input) {
            Ok(Extract::Ready(out)) => Ok(Extract::Ready(Ok(out))),
            Ok(Extract::Incomplete(future)) => Ok(Extract::Incomplete(FallibleFuture(future))),
            Err(err) => Ok(Extract::Ready(Err(err))),
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct FallibleFuture<F>(F);

impl<F> Future for FallibleFuture<F>
where
    F: Future,
{
    type Item = Result<F::Item, F::Error>;
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0.poll() {
            Ok(Async::Ready(out)) => Ok(Async::Ready(Ok(out))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Ok(Async::Ready(Err(err))),
        }
    }
}

#[derive(Debug)]
pub struct EitherOr<L, R> {
    left: L,
    right: R,
}

impl<L, R> Extractor for EitherOr<L, R>
where
    L: Extractor,
    R: Extractor,
{
    type Output = Either<L::Output, R::Output>;
    type Error = Error;
    type Future = EitherOrFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.left.extract(input) {
            Ok(Extract::Ready(out)) => Ok(Extract::Ready(Either::Left(out))),
            Ok(Extract::Incomplete(left)) => match self.right.extract(input) {
                Ok(Extract::Ready(out)) => Ok(Extract::Ready(Either::Right(out))),
                Ok(Extract::Incomplete(right)) => Ok(Extract::Incomplete(EitherOrFuture::Both(
                    left.map(Either::Left as fn(L::Output) -> _)
                        .map_err(Into::into as fn(L::Error) -> Error)
                        .select(
                            right
                                .map(Either::Right as fn(R::Output) -> _)
                                .map_err(Into::into as fn(R::Error) -> Error),
                        ),
                ))),
                Err(..) => Ok(Extract::Incomplete(EitherOrFuture::Left(left))),
            },
            Err(..) => match self.right.extract(input).map_err(Into::into)? {
                Extract::Ready(out) => Ok(Extract::Ready(Either::Right(out))),
                Extract::Incomplete(right) => Ok(Extract::Incomplete(EitherOrFuture::Right(right))),
            },
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
pub enum EitherOrFuture<L: Future, R: Future>
where
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    Left(L),
    Right(R),
    Both(
        future::Select<
            future::MapErr<
                future::Map<L, fn(L::Item) -> Either<L::Item, R::Item>>,
                fn(L::Error) -> Error,
            >,
            future::MapErr<
                future::Map<R, fn(R::Item) -> Either<L::Item, R::Item>>,
                fn(R::Error) -> Error,
            >,
        >,
    ),
}

impl<L, R> Future for EitherOrFuture<L, R>
where
    L: Future,
    R: Future,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    type Item = Either<L::Item, R::Item>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            EitherOrFuture::Both(ref mut future) => future
                .poll()
                .map(|x| x.map(|(out, _next)| out))
                .map_err(|(err, _next)| err),
            EitherOrFuture::Left(ref mut left) => {
                left.poll().map(|x| x.map(Either::Left)).map_err(Into::into)
            }
            EitherOrFuture::Right(ref mut right) => right
                .poll()
                .map(|x| x.map(Either::Right))
                .map_err(Into::into),
        }
    }
}

mod tuple {
    use super::*;

    impl Extractor for () {
        type Output = ();
        type Error = Never;
        type Future = Placeholder<Self::Output, Self::Error>;

        #[inline]
        fn extract(&self, _: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            Ok(Extract::Ready(()))
        }
    }

    impl<E> Extractor for (E,)
    where
        E: Extractor,
    {
        type Output = (E::Output,);
        type Error = E::Error;
        #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
        type Future = futures::future::Map<E::Future, fn(E::Output) -> (E::Output,)>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            match self.0.extract(input)? {
                Extract::Ready(out) => Ok(Extract::Ready((out,))),
                Extract::Incomplete(future) => Ok(Extract::Incomplete(
                    future.map((|out| (out,)) as fn(E::Output) -> (E::Output,)),
                )),
            }
        }
    }

    macro_rules! impl_extractor_for_tuple {
        ($Future:ident => ($($T:ident),*)) => {
            impl<$($T),*> Extractor for ($($T),*)
            where
                $( $T: Extractor, )*
            {
                type Output = ($($T::Output),*);
                type Error = Error;
                type Future = $Future<$($T::Future),*>;

                #[allow(nonstandard_style)]
                fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
                    let ($(ref $T),*) = self;
                    $(
                        let $T = match $T.extract(input).map_err(Into::into)? {
                            Extract::Ready(out) => MaybeDone::Ready(out),
                            Extract::Incomplete(future) => MaybeDone::Pending(future),
                        };
                    )*
                    match ($($T),*) {
                        ($(MaybeDone::Ready($T)),*) => Ok(Extract::Ready(($($T),*))),
                        ($($T),*) => Ok(Extract::Incomplete($Future { $($T),* })),
                    }
                }
            }

            #[allow(missing_debug_implementations, nonstandard_style)]
            pub struct $Future<$($T: Future),*> {
                $( $T: MaybeDone<$T>, )*
            }

            impl<$($T),*> $Future<$($T),*>
            where
                $(
                    $T: Future,
                    $T::Error: Into<Error>,
                )*
            {
                fn poll_ready(&mut self) -> Poll<(), Error> {
                    $(
                        futures::try_ready!(self.$T.poll_ready().map_err(Into::into));
                    )*
                    Ok(Async::Ready(()))
                }

                fn erase(&mut self) {
                    $(
                        let _ = self.$T.take_item();
                    )*
                }
            }

            impl<$($T),*> Future for $Future<$($T),*>
            where
                $(
                    $T: Future,
                    $T::Error: Into<Error>,
                )*
            {
                type Item = ($($T::Item),*);
                type Error = Error;

                fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                    match self.poll_ready() {
                        Ok(Async::Ready(())) => {
                            self.erase();
                            let item = ($(self.$T.take_item().expect("the item should be available")),*);
                            Ok(Async::Ready(item))
                        },
                        Ok(Async::NotReady) => Ok(Async::NotReady),
                        Err(err) => {
                            self.erase();
                            Err(err)
                        }
                    }
                }
            }
        };
    }

    impl_extractor_for_tuple!(Join2 => (T1, T2));
    impl_extractor_for_tuple!(Join3 => (T1, T2, T3));
    impl_extractor_for_tuple!(Join4 => (T1, T2, T3, T4));
    impl_extractor_for_tuple!(Join5 => (T1, T2, T3, T4, T5));
    impl_extractor_for_tuple!(Join6 => (T1, T2, T3, T4, T5, T6));
    impl_extractor_for_tuple!(Join7 => (T1, T2, T3, T4, T5, T6, T7));
    impl_extractor_for_tuple!(Join8 => (T1, T2, T3, T4, T5, T6, T7, T8));
    impl_extractor_for_tuple!(Join9 => (T1, T2, T3, T4, T5, T6, T7, T8, T9));
    impl_extractor_for_tuple!(Join10 => (T1, T2, T3, T4, T5, T6, T7, T8, T9, T10));

    #[allow(missing_debug_implementations)]
    enum MaybeDone<F: Future> {
        Ready(F::Item),
        Pending(F),
        Gone,
    }

    impl<F: Future> MaybeDone<F> {
        fn poll_ready(&mut self) -> Poll<(), F::Error> {
            let async_ = match self {
                MaybeDone::Ready(..) => return Ok(Async::Ready(())),
                MaybeDone::Pending(ref mut future) => future.poll()?,
                MaybeDone::Gone => panic!("This future has already polled"),
            };
            match async_ {
                Async::Ready(item) => {
                    *self = MaybeDone::Ready(item);
                    Ok(Async::Ready(()))
                }
                Async::NotReady => Ok(Async::NotReady),
            }
        }

        fn take_item(&mut self) -> Option<F::Item> {
            match std::mem::replace(self, MaybeDone::Gone) {
                MaybeDone::Ready(item) => Some(item),
                _ => None,
            }
        }
    }
}
