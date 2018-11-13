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
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        match self.0.extract(input) {
            Ok(status) => Ok(status.map(|(out,)| (Some(out),), OptionalFuture::Polling)),
            Err(..) => Ok(ExtractStatus::Pending(OptionalFuture::Failed)),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub enum OptionalFuture<F> {
    Polling(F),
    Failed,
}

impl<F, T> Future for OptionalFuture<F>
where
    F: Future<Item = (T,)>,
{
    type Item = (Option<T>,);
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            OptionalFuture::Polling(ref mut future) => match future.poll() {
                Ok(Async::Ready((out,))) => Ok(Async::Ready((Some(out),))),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(..) => Ok(Async::Ready((None,))),
            },
            OptionalFuture::Failed => Ok(Async::Ready((None,))),
        }
    }
}
