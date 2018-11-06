use super::*;

use std::sync::Arc;

#[derive(Debug)]
pub struct Map<E, F> {
    pub(super) extractor: E,
    pub(super) f: Arc<F>,
}

impl<E, F, T, R> Extractor for Map<E, F>
where
    E: Extractor<Output = (T,)>,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    type Output = (R,);
    type Error = E::Error;
    type Future = MapFuture<E::Future, F>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.extractor.extract(input)? {
            Extract::Ready((out,)) => Ok(Extract::Ready(((*self.f)(out),))),
            Extract::Incomplete(future) => Ok(Extract::Incomplete(MapFuture {
                future,
                f: self.f.clone(),
            })),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct MapFuture<Fut, F> {
    future: Fut,
    f: Arc<F>,
}

impl<Fut, F, T, R> Future for MapFuture<Fut, F>
where
    Fut: Future<Item = (T,)>,
    F: Fn(T) -> R,
{
    type Item = (R,);
    type Error = Fut::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let (out,) = futures::try_ready!(self.future.poll());
        Ok(Async::Ready(((*self.f)(out),)))
    }
}
