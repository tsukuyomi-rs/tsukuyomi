//! Compatible layer with `futures`.

use {
    crate::{core::Never, error::Error, input::Input},
    std::{fmt, marker::PhantomData},
};

#[doc(no_inline)]
pub use futures01::{Async, Poll};

#[derive(Debug)]
pub struct Context<'task> {
    pub input: &'task mut Input<'task>,
    _priv: (),
}

impl<'task> Context<'task> {
    pub(crate) fn new(input: &'task mut Input<'task>) -> Self {
        Self { input, _priv: () }
    }
}

/// A trait representing the asynchronous computation in Tsukuyomi.
///
/// The signature of this trait is intentionally imitates [`Future`],
/// but thereare the following differences:
///
/// * The implementors can access the request context (`&mut Input`) directly
///   through the reference to `Context` passed as an argument of `poll_ready`.
/// * The error type must implements the conversion into `Error`.
///
/// [`Future`]: https://docs.rs/futures/0.1/futures/trait.Future.html
pub trait Future {
    type Output;
    type Error: Into<Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error>;
}

/// A wrapper struct for providing the implementation of [`Future`] to the type
/// that implements [`Future`][Future01].
///
/// [`Future`]: ./trait.Future.html
/// [Future01]: https://docs.rs/futures/0.1/futures/trait.Future.html
#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct Compat01<F>(F);

impl<F> From<F> for Compat01<F>
where
    F: futures01::Future,
    F::Error: Into<Error>,
{
    fn from(future: F) -> Self {
        Compat01(future)
    }
}

impl<F> Future for Compat01<F>
where
    F: futures01::Future,
    F::Error: Into<Error>,
{
    type Output = F::Item;
    type Error = F::Error;

    #[inline]
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        self.0.poll()
    }
}

/// A function to create a `Future` from a function that represents the polling.
pub fn poll_fn<Op, T, E>(op: Op) -> PollFn<Op>
where
    Op: FnMut(&mut Context<'_>) -> Poll<T, E>,
    E: Into<Error>,
{
    PollFn(op)
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct PollFn<Op>(Op);

impl<Op, T, E> Future for PollFn<Op>
where
    Op: FnMut(&mut Context<'_>) -> Poll<T, E>,
    E: Into<Error>,
{
    type Output = T;
    type Error = E;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        (self.0)(cx)
    }
}

/// A function to create a `Future` from a function that produces a `Future`.
pub fn lazy<Op, R>(op: Op) -> Lazy<Op, R>
where
    Op: FnOnce(&mut Context<'_>) -> R,
    R: Future,
{
    Lazy(LazyInner::First(Some(op)))
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct Lazy<Op, R>(LazyInner<Op, R>);

#[derive(Debug)]
enum LazyInner<Op, R> {
    First(Option<Op>),
    Second(R),
}

impl<Op, R> Future for Lazy<Op, R>
where
    Op: FnOnce(&mut Context<'_>) -> R,
    R: Future,
{
    type Output = R::Output;
    type Error = R::Error;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        loop {
            self.0 = match self.0 {
                LazyInner::First(ref mut op) => LazyInner::Second((op.take().unwrap())(cx)),
                LazyInner::Second(ref mut future) => return future.poll_ready(cx),
            }
        }
    }
}

/// An enum that represents arbitrary results that may not be completed.
#[derive(Debug)]
#[must_use = "the internal future do nothing unless polled."]
pub enum MaybeFuture<F: Future> {
    Ready(Result<F::Output, F::Error>),
    Future(F),
}

impl<F: Future> From<F> for MaybeFuture<F> {
    fn from(future: F) -> Self {
        MaybeFuture::Future(future)
    }
}

impl<F: Future> MaybeFuture<F> {
    pub fn is_ready(&self) -> bool {
        match self {
            MaybeFuture::Ready(..) => true,
            MaybeFuture::Future(..) => false,
        }
    }

    pub fn ok(ok: F::Output) -> Self {
        MaybeFuture::Ready(Ok(ok))
    }

    pub fn err(err: F::Error) -> Self {
        MaybeFuture::Ready(Err(err))
    }

    pub fn map_ok<Op, T>(self, op: Op) -> MaybeFuture<MapOk<F, Op>>
    where
        Op: FnOnce(F::Output) -> T,
    {
        match self {
            MaybeFuture::Ready(ready) => MaybeFuture::Ready(ready.map(op)),
            MaybeFuture::Future(future) => MaybeFuture::Future(MapOk(future, Some(op))),
        }
    }

    pub fn map_err<Op, U>(self, op: Op) -> MaybeFuture<MapErr<F, Op>>
    where
        Op: FnOnce(F::Error) -> U,
        U: Into<Error>,
    {
        match self {
            MaybeFuture::Ready(ready) => MaybeFuture::Ready(ready.map_err(op)),
            MaybeFuture::Future(future) => MaybeFuture::Future(MapErr(future, Some(op))),
        }
    }

    pub fn map_result<Op, T, U>(self, op: Op) -> MaybeFuture<MapResult<F, Op>>
    where
        Op: FnOnce(Result<F::Output, F::Error>) -> Result<T, U>,
        U: Into<Error>,
    {
        match self {
            MaybeFuture::Ready(ready) => MaybeFuture::Ready(op(ready)),
            MaybeFuture::Future(future) => MaybeFuture::Future(MapResult(future, Some(op))),
        }
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct MapOk<Fut, Op>(Fut, Option<Op>);

impl<Fut, Op, T> Future for MapOk<Fut, Op>
where
    Fut: Future,
    Op: FnOnce(Fut::Output) -> T,
{
    type Output = T;
    type Error = Fut::Error;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        let result = match self.0.poll_ready(cx) {
            Ok(Async::Ready(ok)) => Ok(ok),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => Err(err),
        };
        let op = self.1.take().expect("the future has already polled");
        result.map(op).map(Async::Ready)
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct MapErr<Fut, Op>(Fut, Option<Op>);

impl<Fut, Op, U> Future for MapErr<Fut, Op>
where
    Fut: Future,
    Op: FnOnce(Fut::Error) -> U,
    U: Into<Error>,
{
    type Output = Fut::Output;
    type Error = U;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        let result = match self.0.poll_ready(cx) {
            Ok(Async::Ready(ok)) => Ok(ok),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => Err(err),
        };
        let op = self.1.take().expect("the future has already polled");
        result.map_err(op).map(Async::Ready)
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub struct MapResult<Fut, Op>(Fut, Option<Op>);

impl<Fut, Op, T, U> Future for MapResult<Fut, Op>
where
    Fut: Future,
    Op: FnOnce(Result<Fut::Output, Fut::Error>) -> Result<T, U>,
    U: Into<Error>,
{
    type Output = T;
    type Error = U;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        let result = match self.0.poll_ready(cx) {
            Ok(Async::Ready(ok)) => Ok(ok),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => Err(err),
        };
        let op = self.1.take().expect("the future has already polled");
        op(result).map(Async::Ready)
    }
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

impl<T, E> Future for NeverFuture<T, E>
where
    E: Into<Error>,
{
    type Output = T;
    type Error = E;

    #[inline]
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        match self.never {}
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub enum MaybeDone<F: Future> {
    Ready(F::Output),
    Pending(F),
    Gone,
}

impl<F: Future> MaybeDone<F> {
    pub fn take_item(&mut self) -> Option<F::Output> {
        match std::mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Ready(output) => Some(output),
            _ => None,
        }
    }
}

impl<F: Future> Future for MaybeDone<F> {
    type Output = ();
    type Error = F::Error;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output, Self::Error> {
        let polled = match self {
            MaybeDone::Ready(..) => return Ok(Async::Ready(())),
            MaybeDone::Pending(ref mut future) => future.poll_ready(cx)?,
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
