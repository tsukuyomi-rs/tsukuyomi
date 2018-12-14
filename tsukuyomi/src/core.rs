//! Definition of commonly used components.

use {
    futures01::{Async, Future, Poll},
    std::{error::Error as StdError, fmt, marker::PhantomData},
};

/// A helper type which emulates the standard `never_type` (`!`).
#[allow(clippy::empty_enum)]
#[derive(Debug)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl StdError for Never {
    fn description(&self) -> &str {
        match *self {}
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match *self {}
    }
}

/// A trait representing the conversion from the arbitrary types.
///
/// This trait is an emulation of the standard [`TryFrom`].
///
/// [`TryFrom`]: https://doc.rust-lang.org/nightly/std/convert/trait.TryFrom.html
pub trait TryFrom<T>: Sized {
    type Error: Into<failure::Error>;

    fn try_from(value: T) -> Result<Self, Self::Error>;
}

pub trait TryInto<T> {
    type Error: Into<failure::Error>;

    fn try_into(self) -> Result<T, Self::Error>;
}

impl<T, U> TryInto<U> for T
where
    U: TryFrom<T>,
{
    type Error = <U as TryFrom<T>>::Error;

    #[inline]
    fn try_into(self) -> Result<U, Self::Error> {
        U::try_from(self)
    }
}

/// A pair of structs representing arbitrary chain structure.
#[derive(Debug, Clone)]
pub struct Chain<L, R> {
    pub(crate) left: L,
    pub(crate) right: R,
}

impl<L, R> Chain<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

#[macro_export]
macro_rules! chain {
    ($e:expr) => ( $e );
    ($e:expr, ) => ( $e );
    ($e1:expr, $e2:expr) => ( $crate::core::Chain::new($e1, $e2) );
    ($e1:expr, $e2:expr, $($t:expr),*) => {
        $crate::core::Chain::new($e1, chain!($e2, $($t),*))
    };
    ($e1:expr, $e2:expr, $($t:expr,)+) => ( chain!{ $e1, $e2, $($t),+ } );
}

/// A helper struct representing a `Future` that will be *never* constructed.
#[must_use = "futures do nothing unless polled."]
pub struct NeverFuture<T, E> {
    never: Never,
    _marker: PhantomData<fn() -> (T, E)>,
}

impl<T, E> fmt::Debug for NeverFuture<T, E> {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.never {}
    }
}

impl<T, E> Future for NeverFuture<T, E> {
    type Item = T;
    type Error = E;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.never {}
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub enum MaybeDone<F: Future> {
    Ready(F::Item),
    Pending(F),
    Gone,
}

impl<F: Future> MaybeDone<F> {
    pub fn take_item(&mut self) -> Option<F::Item> {
        match std::mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Ready(output) => Some(output),
            _ => None,
        }
    }
}

impl<F: Future> Future for MaybeDone<F> {
    type Item = ();
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let polled = match self {
            MaybeDone::Ready(..) => return Ok(Async::Ready(())),
            MaybeDone::Pending(ref mut future) => future.poll()?,
            MaybeDone::Gone => panic!("This future has already polled"),
        };
        match polled {
            Async::Ready(output) => {
                *self = MaybeDone::Ready(output);
                Ok(Async::Ready(()))
            }
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}
