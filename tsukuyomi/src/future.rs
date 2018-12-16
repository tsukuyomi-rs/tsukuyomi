use crate::{error::Error, input::Input, util::Either};

#[doc(no_inline)]
pub use futures01::{Async, Poll};

/// A trait that abstracts the general asynchronous tasks within the framework.
pub trait TryFuture {
    type Ok;
    type Error: Into<Error>;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error>;
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
