#![allow(missing_docs)]

//! A compatible layer for preparing the migration to the standard task system.
//!
//! The components within this module are intentionally named to correspond to those in
//! `std::future` and `std::task`.

use futures;

#[cfg(feature = "nightly")]
use std::task as stdtask;

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

impl Into<Result<futures::Async<()>, ()>> for Poll<()> {
    fn into(self) -> Result<futures::Async<()>, ()> {
        match self {
            Poll::Ready(()) => Ok(futures::Async::Ready(())),
            Poll::Pending => Ok(futures::Async::NotReady),
        }
    }
}

#[cfg(feature = "nightly")]
impl<T> From<stdtask::Poll<T>> for Poll<T> {
    fn from(p: stdtask::Poll<T>) -> Poll<T> {
        match p {
            stdtask::Poll::Ready(v) => Poll::Ready(v),
            stdtask::Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(feature = "nightly")]
impl<T> Into<stdtask::Poll<T>> for Poll<T> {
    fn into(self) -> stdtask::Poll<T> {
        match self {
            Poll::Ready(v) => stdtask::Poll::Ready(v),
            Poll::Pending => stdtask::Poll::Pending,
        }
    }
}

pub trait Future {
    type Output;

    fn poll(&mut self) -> Poll<Self::Output>;
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

#[macro_export]
macro_rules! ready {
    ($e:expr) => {{
        use $crate::future::Poll;
        match $e {
            Poll::Ready(x) => x,
            Poll::Pending => return Poll::Pending,
        }
    }};
}

