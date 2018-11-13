use super::*;

#[derive(Debug)]
pub struct Map<E, F> {
    pub(super) extractor: E,
    pub(super) f: F,
}

impl<E, F, R> Extractor for Map<E, F>
where
    E: Extractor,
    F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
{
    type Output = (R,);
    type Error = E::Error;
    type Future = MapFuture<E::Future, F>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        self.extractor.extract(input).map(|status| {
            status.map(
                |args| (self.f.call(args),),
                |future| MapFuture {
                    future,
                    f: self.f.clone(),
                },
            )
        })
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct MapFuture<Fut, F> {
    future: Fut,
    f: F,
}

impl<Fut, F, R> Future for MapFuture<Fut, F>
where
    Fut: Future,
    Fut::Item: Tuple,
    F: Func<Fut::Item, Out = R>,
{
    type Item = (R,);
    type Error = Fut::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll().map(|x| x.map(|out| (self.f.call(out),)))
    }
}
