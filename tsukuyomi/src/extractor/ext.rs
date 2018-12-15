use {
    super::Extractor,
    crate::{
        core::Chain, //
        generic::{Combine, Func},
    },
};

pub trait ExtractorExt: Extractor + Sized {
    fn chain<E>(self, other: E) -> Chain<Self, E>
    where
        Self: Sized,
        E: Extractor,
        Self::Output: Combine<E::Output> + Send + 'static,
        E::Output: Send + 'static,
    {
        Chain::new(self, other)
    }

    fn optional<T>(self) -> self::optional::Optional<Self, T>
    where
        Self: Extractor<Output = (T,)>,
        T: Send + 'static,
    {
        self::optional::Optional {
            extractor: self,
            _marker: std::marker::PhantomData,
        }
    }

    fn either_or<E>(self, other: E) -> self::either_or::EitherOr<Self, E>
    where
        E: Extractor<Output = Self::Output>,
        E::Error: 'static,
        Self::Output: Send + 'static,
        Self::Error: 'static,
    {
        self::either_or::EitherOr {
            left: self,
            right: other,
        }
    }

    fn map<F>(self, f: F) -> self::map::Map<Self, F>
    where
        F: Func<Self::Output> + Clone + Send + 'static,
    {
        self::map::Map { extractor: self, f }
    }
}

impl<E: Extractor> ExtractorExt for E {}

mod chain {
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
}

mod optional {
    use {
        crate::{core::Never, extractor::Extractor, input::Input},
        futures01::Future,
    };

    #[derive(Debug)]
    pub struct Optional<E, T> {
        pub(super) extractor: E,
        pub(super) _marker: std::marker::PhantomData<fn() -> T>,
    }

    impl<E, T> Extractor for Optional<E, T>
    where
        E: Extractor<Output = (T,)>,
        T: Send + 'static,
    {
        type Output = (Option<T>,);
        type Error = Never;
        type Future = Box<dyn Future<Item = Self::Output, Error = Self::Error> + Send + 'static>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            Box::new(
                self.extractor
                    .extract(input)
                    .then(|result| Ok((result.ok().map(|(x,)| x),))),
            )
        }
    }
}

mod either_or {
    use {
        crate::{error::Error, extractor::Extractor, input::Input},
        futures01::{future::Either, Future},
    };

    #[derive(Debug)]
    pub struct EitherOr<L, R> {
        pub(super) left: L,
        pub(super) right: R,
    }

    impl<L, R> Extractor for EitherOr<L, R>
    where
        L: Extractor,
        L::Output: Send + 'static,
        L::Error: 'static,
        R: Extractor<Output = L::Output>,
        R::Error: 'static,
    {
        type Output = L::Output;
        type Error = Error;
        type Future = Box<dyn Future<Item = Self::Output, Error = Error> + Send + 'static>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            let left = self.left.extract(input);
            let right = self.right.extract(input);
            Box::new(left.select2(right).then(|result| match result {
                Ok(Either::A((a, _))) => Either::A(futures01::future::ok(a)),
                Ok(Either::B((b, _))) => Either::A(futures01::future::ok(b)),
                Err(Either::A((_, b))) => Either::B(Either::B(b.map_err(Into::into))),
                Err(Either::B((_, a))) => Either::B(Either::A(a.map_err(Into::into))),
            }))
        }
    }
}

mod map {
    use {
        crate::{extractor::Extractor, generic::Func, input::Input},
        futures01::Future,
    };

    #[derive(Debug)]
    pub struct Map<E, F> {
        pub(super) extractor: E,
        pub(super) f: F,
    }

    impl<E, F> Extractor for Map<E, F>
    where
        E: Extractor,
        E::Error: 'static,
        F: Func<E::Output> + Clone + Send + 'static,
        F::Out: 'static,
    {
        type Output = (F::Out,);
        type Error = E::Error;
        type Future = Box<dyn Future<Item = Self::Output, Error = Self::Error> + Send + 'static>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            let f = self.f.clone();
            Box::new(
                self.extractor
                    .extract(input)
                    .map(move |args| (f.call(args),)),
            )
        }
    }
}
