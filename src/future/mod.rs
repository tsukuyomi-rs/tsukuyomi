#![allow(missing_docs)]

//! A compatible layer for preparing the migration to the standard task system.
//!
//! The components within this module are intentionally named to correspond to those in
//! `std::future` and `std::task`.

pub mod futures01;

pub use self::futures01::{FutureFromCompat01Ext, FutureToCompat01Ext};

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

pub trait Future {
    type Output;

    fn poll(&mut self) -> Poll<Self::Output>;
}

/// A helper macro for extracting the successful value from a `Poll<T>`.
#[macro_export]
macro_rules! ready_compat {
    ($e:expr) => {{
        match $crate::future::Poll::from($e) {
            $crate::future::Poll::Ready(x) => x,
            $crate::future::Poll::Pending => return $crate::future::Poll::Pending,
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
