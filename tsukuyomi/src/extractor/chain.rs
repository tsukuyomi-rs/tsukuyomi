use {
    super::Extractor,
    crate::{
        common::Chain,
        error::Error,
        future::{Async, Future, MaybeDone, MaybeFuture, Poll},
        generic::{Combine, Tuple},
        input::Input,
    },
};

impl<L, R> Extractor for Chain<L, R>
where
    L: Extractor,
    R: Extractor,
    L::Output: Combine<R::Output> + Send + 'static,
    R::Output: Send + 'static,
{
    type Output = <L::Output as Combine<R::Output>>::Out;
    type Future = ChainFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        let left = match self.left.extract(input) {
            MaybeFuture::Ready(Ok(output)) => MaybeDone::Ready(output),
            MaybeFuture::Ready(Err(e)) => return MaybeFuture::err(e.into()),
            MaybeFuture::Future(future) => MaybeDone::Pending(future),
        };
        let right = match self.right.extract(input) {
            MaybeFuture::Ready(Ok(output)) => MaybeDone::Ready(output),
            MaybeFuture::Ready(Err(e)) => return MaybeFuture::err(e.into()),
            MaybeFuture::Future(future) => MaybeDone::Pending(future),
        };
        match (left, right) {
            (MaybeDone::Ready(left), MaybeDone::Ready(right)) => {
                MaybeFuture::ok(left.combine(right))
            }
            (left, right) => MaybeFuture::from(ChainFuture { left, right }),
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
    L::Output: Tuple + Combine<R::Output>,
    R::Output: Tuple,
{
    fn poll_ready(&mut self, cx: &mut crate::future::Context<'_>) -> Poll<(), Error> {
        futures01::try_ready!(self.left.poll_ready(cx).map_err(Into::into));
        futures01::try_ready!(self.right.poll_ready(cx).map_err(Into::into));
        Ok(Async::Ready(()))
    }
}

impl<L: Future, R: Future> Future for ChainFuture<L, R>
where
    L::Output: Tuple + Combine<R::Output>,
    R::Output: Tuple,
{
    type Output = <L::Output as Combine<R::Output>>::Out;
    type Error = Error;

    fn poll_ready(
        &mut self,
        cx: &mut crate::future::Context<'_>,
    ) -> Poll<Self::Output, Self::Error> {
        match self.poll_ready(cx) {
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
