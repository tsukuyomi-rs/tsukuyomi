use {
    super::Extractor,
    crate::{
        core::{Chain, MaybeDone},
        error::Error,
        generic::{Combine, Tuple},
        input::Input,
    },
    futures01::{Async, Future, Poll},
};

impl<L, R> Extractor for Chain<L, R>
where
    L: Extractor,
    R: Extractor,
    L::Output: Combine<R::Output> + Send + 'static,
    R::Output: Send + 'static,
{
    type Output = <L::Output as Combine<R::Output>>::Out;
    type Error = Error;
    type Future = ChainFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Self::Future {
        let left = self.left.extract(input);
        let right = self.right.extract(input);
        ChainFuture {
            left: MaybeDone::Pending(left),
            right: MaybeDone::Pending(right),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct ChainFuture<L: Future, R: Future> {
    left: MaybeDone<L>,
    right: MaybeDone<R>,
}

impl<L: Future, R: Future> ChainFuture<L, R>
where
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    fn poll_ready(&mut self) -> Poll<(), Error> {
        futures01::try_ready!(self.left.poll().map_err(Into::into));
        futures01::try_ready!(self.right.poll().map_err(Into::into));
        Ok(Async::Ready(()))
    }
}

impl<L: Future, R: Future> Future for ChainFuture<L, R>
where
    L::Item: Tuple + Combine<R::Item>,
    R::Item: Tuple,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    type Item = <L::Item as Combine<R::Item>>::Out;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_ready() {
            Ok(Async::Ready(())) => {
                let left = self.left.take_item().expect("the item should be available");
                let right = self
                    .right
                    .take_item()
                    .expect("the item should be available");
                Ok(Async::Ready(left.combine(right)))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                let _ = self.left.take_item();
                let _ = self.right.take_item();
                Err(err)
            }
        }
    }
}
