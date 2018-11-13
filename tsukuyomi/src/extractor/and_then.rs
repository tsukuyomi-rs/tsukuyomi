use super::*;

#[derive(Debug)]
pub struct AndThen<E, F> {
    pub(super) extractor: E,
    pub(super) f: F,
}

impl<E, F, R> Extractor for AndThen<E, F>
where
    E: Extractor,
    F: Func<E::Output, Out = R> + Clone + Send + Sync + 'static,
    R: IntoFuture + 'static,
    R::Error: Into<Error>,
    R::Future: Send + 'static,
{
    type Output = (R::Item,);
    type Error = Error;
    type Future = AndThenFuture<E::Future, R, F>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        match self.extractor.extract(input).map_err(Into::into)? {
            ExtractStatus::Canceled(output) => Ok(ExtractStatus::Canceled(output)),
            ExtractStatus::Ready(arg) => {
                let future = self.f.call(arg).into_future();
                Ok(ExtractStatus::Pending(AndThenFuture {
                    state: AndThenState::Second(future),
                }))
            }
            ExtractStatus::Pending(future) => Ok(ExtractStatus::Pending(AndThenFuture {
                state: AndThenState::First(future, self.f.clone()),
            })),
        }
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct AndThenFuture<F1, F2, F>
where
    F1: Future,
    F1::Item: Tuple,
    F1::Error: Into<Error>,
    F2: IntoFuture,
    F2::Error: Into<Error>,
    F: Func<F1::Item, Out = F2>,
{
    state: AndThenState<F1, F2::Future, F>,
}

#[allow(missing_debug_implementations)]
enum AndThenState<F1, F2, F> {
    First(F1, F),
    Second(F2),
    Empty,
}

impl<F1, F2, F> Future for AndThenFuture<F1, F2, F>
where
    F1: Future,
    F1::Item: Tuple,
    F1::Error: Into<Error>,
    F2: IntoFuture,
    F2::Error: Into<Error>,
    F: Func<F1::Item, Out = F2>,
{
    type Item = (F2::Item,);
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_future = match self.state {
                AndThenState::First(ref mut f1, ref f) => match f1.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(ok)) => Ok(f.call(ok)),
                    Err(err) => Err(err),
                },
                AndThenState::Second(ref mut f2) => {
                    return f2.poll().map(|x| x.map(|out| (out,))).map_err(Into::into)
                }
                AndThenState::Empty => panic!("This future has already polled."),
            };

            match next_future {
                Ok(future) => {
                    self.state = AndThenState::Second(future.into_future());
                    continue;
                }
                Err(err) => {
                    self.state = AndThenState::Empty;
                    return Err(err.into());
                }
            }
        }
    }
}
