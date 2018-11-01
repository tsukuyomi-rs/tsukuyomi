use super::*;

#[derive(Debug)]
pub struct Or<L, R> {
    pub(super) left: L,
    pub(super) right: R,
}

impl<L, R> Extractor for Or<L, R>
where
    L: Extractor,
    R: Extractor<Output = L::Output>,
{
    type Output = L::Output;
    type Error = Error;
    type Future = OrFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.left.extract(input) {
            Ok(Extract::Ready(out)) => Ok(Extract::Ready(out)),
            Ok(Extract::Incomplete(left)) => match self.right.extract(input) {
                Ok(Extract::Ready(out)) => Ok(Extract::Ready(out)),
                Ok(Extract::Incomplete(right)) => Ok(Extract::Incomplete(OrFuture::Both(
                    left.map_err(Into::into as fn(L::Error) -> Error)
                        .select(right.map_err(Into::into as fn(R::Error) -> Error)),
                ))),
                Err(..) => Ok(Extract::Incomplete(OrFuture::Left(left))),
            },
            Err(..) => match self.right.extract(input).map_err(Into::into)? {
                Extract::Ready(out) => Ok(Extract::Ready(out)),
                Extract::Incomplete(right) => Ok(Extract::Incomplete(OrFuture::Right(right))),
            },
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter, type_complexity))]
pub enum OrFuture<L, R>
where
    L: Future,
    R: Future<Item = L::Item>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    Left(L),
    Right(R),
    Both(
        future::Select<
            future::MapErr<L, fn(L::Error) -> Error>,
            future::MapErr<R, fn(R::Error) -> Error>,
        >,
    ),
}

impl<L, R> Future for OrFuture<L, R>
where
    L: Future,
    R: Future<Item = L::Item>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    type Item = L::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            OrFuture::Both(ref mut future) => future
                .poll()
                .map(|x| x.map(|(out, _next)| out))
                .map_err(|(err, _next)| err),
            OrFuture::Left(ref mut left) => left.poll().map_err(Into::into),
            OrFuture::Right(ref mut right) => right.poll().map_err(Into::into),
        }
    }
}
