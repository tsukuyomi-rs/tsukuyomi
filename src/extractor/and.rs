use super::*;

#[derive(Debug)]
pub struct And<L, R> {
    pub(super) left: L,
    pub(super) right: R,
}

impl<L, R> Extractor for And<L, R>
where
    L: Extractor,
    R: Extractor,
    L::Output: Combine<R::Output>,
{
    type Output = <L::Output as Combine<R::Output>>::Out;
    type Error = Error;
    type Future = AndFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        let left = match self.left.extract(input).map_err(Into::into)? {
            Extract::Ready(out) => MaybeDone::Ready(out),
            Extract::Incomplete(future) => MaybeDone::Pending(future),
        };
        let right = match self.right.extract(input).map_err(Into::into)? {
            Extract::Ready(out) => MaybeDone::Ready(out),
            Extract::Incomplete(future) => MaybeDone::Pending(future),
        };

        match (left, right) {
            (MaybeDone::Ready(left), MaybeDone::Ready(right)) => {
                Ok(Extract::Ready(left.combine(right)))
            }
            (left, right) => Ok(Extract::Incomplete(AndFuture { left, right })),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct AndFuture<L: Future, R: Future> {
    left: MaybeDone<L>,
    right: MaybeDone<R>,
}

impl<L: Future, R: Future> AndFuture<L, R>
where
    L::Error: Into<Error>,
    R::Error: Into<Error>,
    L::Item: Tuple + Combine<R::Item>,
    R::Item: Tuple,
{
    fn poll_ready(&mut self) -> Poll<(), Error> {
        futures::try_ready!(self.left.poll_ready().map_err(Into::into));
        futures::try_ready!(self.right.poll_ready().map_err(Into::into));
        Ok(Async::Ready(()))
    }

    fn erase(&mut self) {
        let _ = self.left.take_item();
        let _ = self.right.take_item();
    }
}

impl<L: Future, R: Future> Future for AndFuture<L, R>
where
    L::Error: Into<Error>,
    R::Error: Into<Error>,
    L::Item: Tuple + Combine<R::Item>,
    R::Item: Tuple,
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
                self.erase();
                Err(err)
            }
        }
    }
}

#[allow(missing_debug_implementations)]
enum MaybeDone<F: Future> {
    Ready(F::Item),
    Pending(F),
    Gone,
}

impl<F: Future> MaybeDone<F> {
    fn poll_ready(&mut self) -> Poll<(), F::Error> {
        let async_ = match self {
            MaybeDone::Ready(..) => return Ok(Async::Ready(())),
            MaybeDone::Pending(ref mut future) => future.poll()?,
            MaybeDone::Gone => panic!("This future has already polled"),
        };
        match async_ {
            Async::Ready(item) => {
                *self = MaybeDone::Ready(item);
                Ok(Async::Ready(()))
            }
            Async::NotReady => Ok(Async::NotReady),
        }
    }

    fn take_item(&mut self) -> Option<F::Item> {
        match std::mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Ready(item) => Some(item),
            _ => None,
        }
    }
}
