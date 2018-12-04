use {
    futures::{Async, Future, IntoFuture, Poll},
    std::{error::Error as StdError, fmt, marker::PhantomData},
};

/// A helper type which emulates the standard `never_type` (`!`).
#[cfg_attr(feature = "cargo-clippy", allow(empty_enum))]
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

/// A trait representing the arbitrary conversion into `Self`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait TryFrom<T>: Sized {
    type Error: Into<failure::Error>;

    fn try_from(value: T) -> Result<Self, Self::Error>;
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

/// An enum that represents arbitrary results that may not be completed.
#[derive(Debug)]
pub enum MaybeFuture<F: Future> {
    Ready(Result<F::Item, F::Error>),
    Future(F),
}

impl<F: Future> From<F> for MaybeFuture<F> {
    fn from(future: F) -> Self {
        MaybeFuture::Future(future)
    }
}

impl<F: Future> MaybeFuture<F> {
    pub fn ok(ok: F::Item) -> Self {
        MaybeFuture::Ready(Ok(ok))
    }

    pub fn err(err: F::Error) -> Self {
        MaybeFuture::Ready(Err(err))
    }

    pub fn is_ready(&self) -> bool {
        match self {
            MaybeFuture::Ready(..) => true,
            MaybeFuture::Future(..) => false,
        }
    }

    pub fn map_ok<T>(
        self,
        f: impl FnOnce(F::Item) -> T,
    ) -> MaybeFuture<impl Future<Item = T, Error = F::Error>> {
        match self {
            MaybeFuture::Ready(result) => MaybeFuture::Ready(result.map(f)),
            MaybeFuture::Future(future) => MaybeFuture::Future(future.map(f)),
        }
    }

    pub fn map_err<U>(
        self,
        f: impl FnOnce(F::Error) -> U,
    ) -> MaybeFuture<impl Future<Item = F::Item, Error = U>> {
        match self {
            MaybeFuture::Ready(result) => MaybeFuture::Ready(result.map_err(f)),
            MaybeFuture::Future(future) => MaybeFuture::Future(future.map_err(f)),
        }
    }

    pub fn map<T, U>(
        self,
        f: impl FnOnce(Result<F::Item, F::Error>) -> Result<T, U>,
    ) -> MaybeFuture<impl Future<Item = T, Error = U>> {
        #[allow(missing_debug_implementations)]
        struct MapFuture<Fut, F>(Fut, Option<F>);

        impl<Fut, F, T, E> Future for MapFuture<Fut, F>
        where
            Fut: Future,
            F: FnOnce(Result<Fut::Item, Fut::Error>) -> Result<T, E>,
        {
            type Item = T;
            type Error = E;

            #[inline]
            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                let result = match self.0.poll() {
                    Ok(Async::Ready(ok)) => Ok(ok),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => Err(err),
                };
                let f = self.1.take().expect("the future has already polled");
                f(result).map(Async::Ready)
            }
        }

        match self {
            MaybeFuture::Ready(result) => MaybeFuture::Ready(f(result)),
            MaybeFuture::Future(future) => MaybeFuture::Future(MapFuture(future, Some(f))),
        }
    }

    pub fn and_then<R>(
        self,
        f: impl FnOnce(F::Item) -> R,
    ) -> MaybeFuture<impl Future<Item = R::Item, Error = F::Error>>
    where
        R: IntoFuture<Error = F::Error>,
    {
        match self {
            MaybeFuture::Ready(result) => {
                MaybeFuture::Future(futures::future::Either::A(result.into_future().and_then(f)))
            }
            MaybeFuture::Future(future) => {
                MaybeFuture::Future(futures::future::Either::B(future.and_then(f)))
            }
        }
    }

    pub fn boxed(
        self,
    ) -> MaybeFuture<Box<dyn Future<Item = F::Item, Error = F::Error> + Send + 'static>>
    where
        F: Send + 'static,
    {
        match self {
            MaybeFuture::Ready(result) => MaybeFuture::Ready(result),
            MaybeFuture::Future(future) => MaybeFuture::Future(Box::new(future)),
        }
    }
}

/// A helper struct representing a `Future` that will be *never* constructed.
#[doc(hidden)]
#[derive(Debug)]
pub struct NeverFuture<T, E> {
    never: Never,
    _marker: PhantomData<fn() -> (T, E)>,
}

impl<T, E> Future for NeverFuture<T, E> {
    type Item = T;
    type Error = E;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.never {}
    }
}
