use {
    super::Extractor,
    crate::{
        core::Never,
        error::Error,
        future::{Async, Future, MaybeFuture, Poll},
        generic::Func,
        input::Input,
    },
};

#[derive(Debug)]
pub struct ExtractorExt<E>(E);

impl<E> From<E> for ExtractorExt<E>
where
    E: Extractor,
{
    fn from(extractor: E) -> Self {
        Self::new(extractor)
    }
}

impl<E> ExtractorExt<E>
where
    E: Extractor,
{
    /// Creates a `Builder` from the specified extractor.
    #[inline]
    pub fn new(extractor: E) -> Self {
        ExtractorExt(extractor)
    }

    /// Returns the inner extractor.
    #[inline]
    pub fn into_inner(self) -> E {
        self.0
    }

    pub fn optional<T>(self) -> ExtractorExt<impl Extractor<Output = (Option<T>,)>>
    where
        E: Extractor<Output = (T,)>,
        T: 'static,
    {
        ExtractorExt {
            0: super::raw(move |input| {
                self.0
                    .extract(input)
                    .map_result(|result| Ok::<_, Never>((result.ok().map(|(x,)| x),)))
            }),
        }
    }

    pub fn either_or<T>(self, other: T) -> ExtractorExt<impl Extractor<Output = E::Output>>
    where
        T: Extractor<Output = E::Output>,
    {
        #[allow(missing_debug_implementations)]
        #[allow(clippy::type_complexity)]
        enum OrFuture<L, R>
        where
            L: Future,
            R: Future<Output = L::Output>,
        {
            Left(L),
            Right(R),
            Both(L, R),
        }

        impl<L, R, T> Future for OrFuture<L, R>
        where
            L: Future<Output = T>,
            R: Future<Output = T>,
        {
            type Output = T;
            type Error = Error;

            fn poll_ready(
                &mut self,
                cx: &mut crate::future::Context<'_>,
            ) -> Poll<Self::Output, Self::Error> {
                match self {
                    OrFuture::Both(ref mut left, ref mut right) => match left.poll_ready(cx) {
                        Ok(Async::NotReady) => match right.poll_ready(cx) {
                            Ok(Async::NotReady) => Ok(Async::NotReady),
                            Ok(Async::Ready(ok)) => Ok(Async::Ready(ok)),
                            Err(err) => Err(err.into()),
                        },
                        Ok(Async::Ready(ok)) => Ok(Async::Ready(ok)),
                        Err(err) => Err(err.into()),
                    },
                    OrFuture::Left(ref mut left) => left.poll_ready(cx).map_err(Into::into),
                    OrFuture::Right(ref mut right) => right.poll_ready(cx).map_err(Into::into),
                }
            }
        }

        let left = self.0;
        let right = other;
        ExtractorExt {
            0: super::raw(move |input| {
                let left = match left.extract(input) {
                    MaybeFuture::Future(future) => future,
                    MaybeFuture::Ready(Ok(left)) => return MaybeFuture::ok(left),
                    MaybeFuture::Ready(Err(..)) => match right.extract(input) {
                        MaybeFuture::Ready(result) => {
                            return MaybeFuture::Ready(result.map_err(Into::into));
                        }
                        MaybeFuture::Future(future) => {
                            return MaybeFuture::Future(OrFuture::Right(future));
                        }
                    },
                };
                match right.extract(input) {
                    MaybeFuture::Ready(Ok(right)) => MaybeFuture::ok(right),
                    MaybeFuture::Ready(Err(..)) => MaybeFuture::Future(OrFuture::Left(left)),
                    MaybeFuture::Future(right) => MaybeFuture::Future(OrFuture::Both(left, right)),
                }
            }),
        }
    }

    pub fn map<F>(self, f: F) -> ExtractorExt<impl Extractor<Output = (F::Out,)>>
    where
        F: Func<E::Output> + Clone + Send + 'static,
    {
        ExtractorExt {
            0: super::raw(move |input| {
                let f = f.clone();
                self.0.extract(input).map_ok(move |args| (f.call(args),))
            }),
        }
    }
}

impl<E> Extractor for ExtractorExt<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        self.0.extract(input)
    }
}
