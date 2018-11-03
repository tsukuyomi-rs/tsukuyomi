use super::*;

#[derive(Debug)]
pub struct Optional<E>(pub(super) E);

impl<E, T> Extractor for Optional<E>
where
    E: Extractor<Output = (T,)>,
{
    type Output = (Option<T>,);
    type Error = Never;
    type Future = OptionalFuture<E::Future>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.0.extract(input) {
            Ok(Extract::Ready((out,))) => Ok(Extract::Ready((Some(out),))),
            Ok(Extract::Incomplete(future)) => Ok(Extract::Incomplete(OptionalFuture(future))),
            Err(..) => Ok(Extract::Ready((None,))),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct OptionalFuture<F>(F);

impl<F, T> Future for OptionalFuture<F>
where
    F: Future<Item = (T,)>,
{
    type Item = (Option<T>,);
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0.poll() {
            Ok(Async::Ready((out,))) => Ok(Async::Ready((Some(out),))),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(..) => Ok(Async::Ready((None,))),
        }
    }
}
