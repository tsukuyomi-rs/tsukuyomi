use super::*;

#[derive(Debug)]
pub struct Or<L, R> {
    pub(super) left: L,
    pub(super) right: R,
}

impl<L, R, T, U> Extractor for Or<L, R>
where
    L: Extractor<Output = (T,)>,
    R: Extractor<Output = (U,)>,
{
    type Output = (Either<T, U>,);
    type Error = Error;
    type Future = OrFuture<L::Future, R::Future, T, U>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        match self.left.extract(input) {
            Ok(Extract::Ready((out,))) => Ok(Extract::Ready((Either::Left(out),))),
            Ok(Extract::Incomplete(left)) => match self.right.extract(input) {
                Ok(Extract::Ready((out,))) => Ok(Extract::Ready((Either::Right(out),))),
                Ok(Extract::Incomplete(right)) => Ok(Extract::Incomplete(OrFuture::Both(
                    left.map((|(out,)| (Either::Left(out),)) as fn((T,)) -> (Either<T, U>,))
                        .map_err(Into::into as fn(L::Error) -> Error)
                        .select(
                            right
                                .map(
                                    (|(out,)| (Either::Right(out),)) as fn((U,)) -> (Either<T, U>,),
                                ).map_err(Into::into as fn(R::Error) -> Error),
                        ),
                ))),
                Err(..) => Ok(Extract::Incomplete(OrFuture::Left(left))),
            },
            Err(..) => match self.right.extract(input).map_err(Into::into)? {
                Extract::Ready((out,)) => Ok(Extract::Ready((Either::Right(out),))),
                Extract::Incomplete(right) => Ok(Extract::Incomplete(OrFuture::Right(right))),
            },
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter, type_complexity))]
pub enum OrFuture<L, R, T, U>
where
    L: Future<Item = (T,)>,
    R: Future<Item = (U,)>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    Left(L),
    Right(R),
    Both(
        future::Select<
            future::MapErr<future::Map<L, fn((T,)) -> (Either<T, U>,)>, fn(L::Error) -> Error>,
            future::MapErr<future::Map<R, fn((U,)) -> (Either<T, U>,)>, fn(R::Error) -> Error>,
        >,
    ),
}

impl<L, R, T, U> Future for OrFuture<L, R, T, U>
where
    L: Future<Item = (T,)>,
    R: Future<Item = (U,)>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    type Item = (Either<T, U>,);
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            OrFuture::Both(ref mut future) => future
                .poll()
                .map(|x| x.map(|(out, _next)| out))
                .map_err(|(err, _next)| err),
            OrFuture::Left(ref mut left) => left
                .poll()
                .map(|x| x.map(|(out,)| (Either::Left(out),)))
                .map_err(Into::into),
            OrFuture::Right(ref mut right) => right
                .poll()
                .map(|x| x.map(|(out,)| (Either::Right(out),)))
                .map_err(Into::into),
        }
    }
}
