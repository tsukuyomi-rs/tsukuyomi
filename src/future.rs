#![allow(missing_docs)]

//! A compatible layer for preparing the migration to the standard task system.
//!
//! The components within this module are intentionally named to correspond to those in
//! `std::future` and `std::task`.

use futures;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Poll<T> {
    Ready(T),
    Pending,
}

impl<T> Poll<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Poll<U> {
        match self {
            Poll::Ready(t) => Poll::Ready(f(t)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T, E> Poll<Result<T, E>> {
    pub fn map_ok<U>(self, f: impl FnOnce(T) -> U) -> Poll<Result<U, E>> {
        self.map(|res| res.map(f))
    }

    pub fn map_err<U>(self, f: impl FnOnce(E) -> U) -> Poll<Result<T, U>> {
        self.map(|res| res.map_err(f))
    }
}

impl<T> From<T> for Poll<T> {
    fn from(x: T) -> Self {
        Poll::Ready(x)
    }
}

impl<T> From<futures::Async<T>> for Poll<T> {
    fn from(a: futures::Async<T>) -> Self {
        match a {
            futures::Async::Ready(v) => Poll::Ready(v),
            futures::Async::NotReady => Poll::Pending,
        }
    }
}

impl<T, E> From<Result<futures::Async<T>, E>> for Poll<Result<T, E>> {
    fn from(a: Result<futures::Async<T>, E>) -> Self {
        match a {
            Ok(futures::Async::Ready(v)) => Poll::Ready(Ok(v)),
            Ok(futures::Async::NotReady) => Poll::Pending,
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl From<Result<futures::Async<()>, ()>> for Poll<()> {
    fn from(a: Result<futures::Async<()>, ()>) -> Self {
        match a {
            Ok(futures::Async::Ready(())) => Poll::Ready(()),
            Ok(futures::Async::NotReady) => Poll::Pending,
            Err(()) => Poll::Ready(()),
        }
    }
}

impl<T> Into<futures::Async<T>> for Poll<T> {
    fn into(self) -> futures::Async<T> {
        match self {
            Poll::Ready(v) => futures::Async::Ready(v),
            Poll::Pending => futures::Async::NotReady,
        }
    }
}

impl<T, E> Into<Result<futures::Async<T>, E>> for Poll<Result<T, E>> {
    fn into(self) -> Result<futures::Async<T>, E> {
        match self {
            Poll::Ready(Ok(v)) => Ok(futures::Async::Ready(v)),
            Poll::Ready(Err(e)) => Err(e),
            Poll::Pending => Ok(futures::Async::NotReady),
        }
    }
}

impl<T, E> Into<Result<futures::Async<T>, E>> for Poll<T> {
    fn into(self) -> Result<futures::Async<T>, E> {
        match self {
            Poll::Ready(v) => Ok(futures::Async::Ready(v)),
            Poll::Pending => Ok(futures::Async::NotReady),
        }
    }
}

pub trait Future {
    type Output;

    fn poll(&mut self) -> Poll<Self::Output>;

    /// Wraps this value into a wrapper struct which implements `futures::Future`.
    ///
    /// This method is available only if the associated type is a `Result`.
    fn compat_01(self) -> CompatFuture01<Self>
    where
        Self: Sized,
        Self::Output: IsResult,
    {
        CompatFuture01(self)
    }
}

impl<F> Future for F
where
    F: futures::Future,
{
    type Output = Result<F::Item, F::Error>;

    fn poll(&mut self) -> Poll<Self::Output> {
        futures::Future::poll(self).into()
    }
}

pub trait IsResult: sealed::Sealed {
    type Ok;
    type Err;

    fn into_result(self) -> Result<Self::Ok, Self::Err>;
}

impl<T, E> IsResult for Result<T, E> {
    type Ok = T;
    type Err = E;

    #[inline(always)]
    fn into_result(self) -> Result<Self::Ok, Self::Err> {
        self
    }
}

mod sealed {
    pub trait Sealed {}

    impl<T, E> Sealed for Result<T, E> {}
}

#[derive(Debug)]
pub struct CompatFuture01<F>(F);

impl<F, T, E> futures::Future for CompatFuture01<F>
where
    F: Future,
    F::Output: IsResult<Ok = T, Err = E>,
{
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        self.0.poll().map(IsResult::into_result).into()
    }
}

/// A helper macro for extracting the successful value from a `Poll<T>`.
#[macro_export]
macro_rules! ready_compat {
    ($e:expr) => {{
        match $crate::future::Poll::from($e) {
            $crate::future::Poll::Ready(x) => x,
            $crate::future::Poll::Pending => return Poll::Pending,
        }
    }};
}

/// A helper macro for extracting the successful value from a `Poll<Result<T, E>>`.
#[macro_export]
macro_rules! try_ready_compat {
    ($e:expr) => {{
        match $crate::future::Poll::from($e) {
            $crate::future::Poll::Ready(Ok(x)) => x,
            $crate::future::Poll::Ready(Err(err)) => return $crate::future::Poll::Ready(Err(Into::into(err))),
            $crate::future::Poll::Pending => return $crate::future::Poll::Pending,
        }
    }};
}
