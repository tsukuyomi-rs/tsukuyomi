use super::*;

#[derive(Debug)]
pub struct Fallible<E>(pub(super) E);

impl<E, T> Extractor for Fallible<E>
where
    E: Extractor<Output = (T,)>,
{
    type Output = (Result<T, E::Error>,);
    type Error = Never;
    type Future = FallibleFuture<E::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.0.extract(input) {
            Ok(Extract::Ready((out,))) => Ok(Extract::Ready((Ok(out),))),
            Ok(Extract::Incomplete(future)) => Ok(Extract::Incomplete(FallibleFuture(future))),
            Err(err) => Ok(Extract::Ready((Err(err),))),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct FallibleFuture<F>(F);

impl<F, T> Future for FallibleFuture<F>
where
    F: Future<Item = (T,)>,
{
    type Item = (Result<T, F::Error>,);
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0.poll() {
            Ok(Async::Ready((out,))) => Ok(Async::Ready((Ok(out),))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Ok(Async::Ready((Err(err),))),
        }
    }
}
