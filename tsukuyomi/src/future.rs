//! Compatible layer of asynchronous tasks used within the framework.

use crate::{error::Error, input::Input, util::Either};

#[doc(no_inline)]
pub use futures01::{try_ready, Async, Poll};

/// A trait that abstracts the general asynchronous tasks within the framework.
pub trait TryFuture {
    type Ok;
    type Error: Into<Error>;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error>;
}

impl<F> TryFuture for Box<F>
where
    F: TryFuture + ?Sized,
{
    type Ok = F::Ok;
    type Error = F::Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        (**self).poll_ready(input)
    }
}

impl<L, R> TryFuture for Either<L, R>
where
    L: TryFuture,
    R: TryFuture,
{
    type Ok = Either<L::Ok, R::Ok>;
    type Error = Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        match self {
            Either::Left(l) => l
                .poll_ready(input)
                .map(|x| x.map(Either::Left))
                .map_err(Into::into),
            Either::Right(r) => r
                .poll_ready(input)
                .map(|x| x.map(Either::Right))
                .map_err(Into::into),
        }
    }
}

pub fn poll_fn<T, E>(
    op: impl FnMut(&mut Input<'_>) -> Poll<T, E>,
) -> impl TryFuture<
    Ok = T, //
    Error = E,
>
where
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct PollFn<F>(F);

    impl<F, T, E> TryFuture for PollFn<F>
    where
        F: FnMut(&mut Input<'_>) -> Poll<T, E>,
        E: Into<Error>,
    {
        type Ok = T;
        type Error = E;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            (self.0)(input)
        }
    }

    PollFn(op)
}

pub fn oneshot<T, E>(
    f: impl FnOnce(&mut Input<'_>) -> Result<T, E>,
) -> impl TryFuture<Ok = T, Error = E>
where
    E: Into<Error>,
{
    let mut f = Some(f);
    self::poll_fn(move |input| (f.take().unwrap())(input).map(Into::into))
}

/// A wrapper struct that provides the implementation of `TryFuture` for
/// implementors of futures 0.1 `Future`.
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

impl<F> TryFuture for Compat01<F>
where
    F: futures01::Future,
    F::Error: Into<Error>,
{
    type Ok = F::Item;
    type Error = F::Error;

    #[inline]
    fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        futures01::Future::poll(&mut self.0)
    }
}

pub trait Futures01CompatExt: futures01::Future + Sized
where
    Self::Error: Into<Error>,
{
    fn compat01(self) -> Compat01<Self> {
        Compat01::from(self)
    }
}

impl<F> Futures01CompatExt for F
where
    F: futures01::Future,
    F::Error: Into<Error>,
{
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled."]
pub enum MaybeDone<F: TryFuture> {
    Ready(F::Ok),
    Pending(F),
    Gone,
}

impl<F: TryFuture> MaybeDone<F> {
    pub fn take_item(&mut self) -> Option<F::Ok> {
        match std::mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Ready(output) => Some(output),
            _ => None,
        }
    }
}

impl<F: TryFuture> TryFuture for MaybeDone<F> {
    type Ok = ();
    type Error = F::Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        let polled = match self {
            MaybeDone::Ready(..) => return Ok(Async::Ready(())),
            MaybeDone::Pending(ref mut future) => future.poll_ready(input)?,
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
