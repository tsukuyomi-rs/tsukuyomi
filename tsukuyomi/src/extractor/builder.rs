use {
    super::Extractor,
    crate::{
        common::{Chain, MaybeFuture, Never},
        error::Error,
        generic::{Combine, Func},
        input::Input,
    },
    futures::{Future, Poll},
};

#[derive(Debug)]
pub struct Builder<E> {
    extractor: E,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Builder<E>
where
    E: Extractor,
{
    /// Creates a `Builder` from the specified extractor.
    #[inline]
    pub fn new(extractor: E) -> Self {
        Self { extractor }
    }

    /// Returns the inner extractor.
    #[inline]
    pub fn into_inner(self) -> E {
        self.extractor
    }

    pub fn optional<T>(self) -> Builder<impl Extractor<Output = (Option<T>,), Error = Never>>
    where
        E: Extractor<Output = (T,)>,
        T: 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                self.extractor
                    .extract(input)
                    .map(|result| Ok((result.ok().map(|(x,)| x),)))
            }),
        }
    }

    pub fn fallible<T>(
        self,
    ) -> Builder<impl Extractor<Output = (Result<T, E::Error>,), Error = Never>>
    where
        E: Extractor<Output = (T,)>,
        T: Send + 'static,
        E::Error: Send + 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                self.extractor
                    .extract(input)
                    .map(|result| Ok((result.map(|(x,)| x),)))
            }),
        }
    }

    pub fn and<T>(
        self,
        other: T,
    ) -> Builder<impl Extractor<Output = <E::Output as Combine<T::Output>>::Out, Error = Error>>
    where
        T: Extractor,
        E::Output: Combine<T::Output> + Send + 'static,
        T::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
        }
    }

    pub fn or<T>(self, other: T) -> Builder<impl Extractor<Output = E::Output, Error = Error>>
    where
        T: Extractor<Output = E::Output>,
    {
        #[allow(missing_debug_implementations)]
        #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
        enum OrFuture<L, R>
        where
            L: Future,
            R: Future<Item = L::Item>,
            L::Error: Into<Error>,
            R::Error: Into<Error>,
        {
            Left(L),
            Right(R),
            Both(
                futures::future::Select<
                    futures::future::MapErr<L, fn(L::Error) -> Error>,
                    futures::future::MapErr<R, fn(R::Error) -> Error>,
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

        let left = self.extractor;
        let right = other;
        Builder {
            extractor: super::raw(move |input| {
                let left = match left.extract(input) {
                    MaybeFuture::Future(future) => future,
                    MaybeFuture::Ready(Ok(left)) => return MaybeFuture::ok(left),
                    MaybeFuture::Ready(Err(..)) => match right.extract(input) {
                        MaybeFuture::Ready(result) => {
                            return MaybeFuture::Ready(result.map_err(Into::into))
                        }
                        MaybeFuture::Future(future) => {
                            return MaybeFuture::Future(OrFuture::Right(future))
                        }
                    },
                };
                match right.extract(input) {
                    MaybeFuture::Ready(Ok(right)) => return MaybeFuture::ok(right),
                    MaybeFuture::Ready(Err(..)) => MaybeFuture::Future(OrFuture::Left(left)),
                    MaybeFuture::Future(right) => MaybeFuture::Future(OrFuture::Both(
                        left.map_err(Into::into as fn(E::Error) -> Error)
                            .select(right.map_err(Into::into as fn(T::Error) -> Error)),
                    )),
                }
            }),
        }
    }

    pub fn map<F>(self, f: F) -> Builder<impl Extractor<Output = (F::Out,), Error = E::Error>>
    where
        F: Func<E::Output> + Clone + Send + Sync + 'static,
    {
        Builder {
            extractor: super::raw(move |input| {
                let f = f.clone();
                self.extractor
                    .extract(input)
                    .map_ok(move |args| (f.call(args),))
            }),
        }
    }
}

impl<E> Extractor for Builder<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        self.extractor.extract(input)
    }
}
