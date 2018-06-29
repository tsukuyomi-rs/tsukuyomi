use futures;

use super::{Future, Poll};

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

impl<F> Future for F
where
    F: futures::Future,
{
    type Output = Result<F::Item, F::Error>;

    fn poll(&mut self) -> Poll<Self::Output> {
        futures::Future::poll(self).into()
    }
}

pub trait FutureFromCompat01Ext: futures::Future {
    fn compat_from_futures01(self) -> CompatFromFuture01<Self>
    where
        Self: Sized,
    {
        CompatFromFuture01(self)
    }
}

impl<F> FutureFromCompat01Ext for F
where
    F: futures::Future,
{
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct CompatFromFuture01<F>(F);

impl<F> From<F> for CompatFromFuture01<F>
where
    F: futures::Future,
{
    fn from(future: F) -> Self {
        CompatFromFuture01(future)
    }
}

impl<F> Future for CompatFromFuture01<F>
where
    F: futures::Future,
{
    type Output = Result<F::Item, F::Error>;

    fn poll(&mut self) -> Poll<Self::Output> {
        futures::Future::poll(&mut self.0).into()
    }
}

pub trait FutureToCompat01Ext<T, E>: Future<Output = Result<T, E>> {
    /// Wraps this value into a wrapper struct which implements `futures::Future`.
    ///
    /// This method is available only if the associated type is a `Result`.
    fn compat_to_futures01(self) -> CompatToFuture01<Self>
    where
        Self: Sized,
    {
        CompatToFuture01(self)
    }
}

impl<F, T, E> FutureToCompat01Ext<T, E> for F
where
    F: Future<Output = Result<T, E>>,
{
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct CompatToFuture01<F>(F);

impl<F, T, E> futures::Future for CompatToFuture01<F>
where
    F: Future<Output = Result<T, E>>,
{
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        self.0.poll().into()
    }
}
