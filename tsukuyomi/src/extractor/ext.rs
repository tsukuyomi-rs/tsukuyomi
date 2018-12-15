use {
    super::Extractor,
    crate::{
        core::Chain, //
        generic::{Combine, Func},
    },
};

pub use self::{either_or::EitherOr, map::Map, optional::Optional};

pub trait ExtractorExt: Extractor + Sized {
    fn chain<E>(self, other: E) -> Chain<Self, E>
    where
        Self: Sized,
        E: Extractor,
        Self::Output: Combine<E::Output>,
    {
        Chain::new(self, other)
    }

    fn optional<T>(self) -> self::optional::Optional<Self, T>
    where
        Self: Extractor<Output = (T,)>,
    {
        self::optional::Optional {
            extractor: self,
            _marker: std::marker::PhantomData,
        }
    }

    fn either_or<E>(self, other: E) -> self::either_or::EitherOr<Self, E>
    where
        E: Extractor<Output = Self::Output>,
    {
        self::either_or::EitherOr {
            left: self,
            right: other,
        }
    }

    fn map<F>(self, f: F) -> self::map::Map<Self, F>
    where
        F: Func<Self::Output> + Clone,
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
        L::Output: Combine<R::Output>,
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
        futures01::{Async, Future, Poll},
    };

    #[derive(Debug)]
    pub struct Optional<E, T> {
        pub(super) extractor: E,
        pub(super) _marker: std::marker::PhantomData<fn() -> T>,
    }

    impl<E, T> Extractor for Optional<E, T>
    where
        E: Extractor<Output = (T,)>,
    {
        type Output = (Option<T>,);
        type Error = Never;
        type Future = OptionalFuture<E::Future>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            OptionalFuture {
                0: self.extractor.extract(input),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct OptionalFuture<Fut>(Fut);

    impl<Fut, T> Future for OptionalFuture<Fut>
    where
        Fut: Future<Item = (T,)>,
    {
        type Item = (Option<T>,);
        type Error = Never;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self.0.poll() {
                Ok(Async::Ready((ok,))) => Ok(Async::Ready((Some(ok),))),
                Err(..) => Ok(Async::Ready((None,))),
                Ok(Async::NotReady) => Ok(Async::NotReady),
            }
        }
    }
}

mod either_or {
    use {
        crate::{error::Error, extractor::Extractor, generic::Tuple, input::Input},
        futures01::{Async, Future, Poll},
    };

    #[derive(Debug)]
    pub struct EitherOr<L, R> {
        pub(super) left: L,
        pub(super) right: R,
    }

    impl<L, R, T: Tuple> Extractor for EitherOr<L, R>
    where
        L: Extractor<Output = T>,
        R: Extractor<Output = T>,
    {
        type Output = T;
        type Error = Error;
        type Future = EitherOrFuture<L::Future, R::Future>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            EitherOrFuture {
                left: Some(self.left.extract(input)),
                right: Some(self.right.extract(input)),
            }
        }
    }

    #[derive(Debug)]
    pub struct EitherOrFuture<L, R> {
        left: Option<L>,
        right: Option<R>,
    }

    impl<L, R, T> Future for EitherOrFuture<L, R>
    where
        L: Future<Item = T>,
        R: Future<Item = T>,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
    {
        type Item = T;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            loop {
                match (&mut self.left, &mut self.right) {
                    (Some(left), Some(right)) => match left.poll() {
                        Ok(Async::NotReady) => match right.poll() {
                            Ok(Async::Ready(right)) => return Ok(Async::Ready(right)),
                            Ok(Async::NotReady) => return Ok(Async::NotReady),
                            Err(..) => {
                                self.right = None;
                                return Ok(Async::NotReady);
                            }
                        },
                        Ok(Async::Ready(left)) => return Ok(Async::Ready(left)),
                        Err(..) => {
                            self.left = None;
                            continue;
                        }
                    },
                    (Some(left), None) => return left.poll().map_err(Into::into),
                    (None, Some(right)) => return right.poll().map_err(Into::into),
                    (None, None) => unreachable!(),
                }
            }
        }
    }
}

mod map {
    use {
        crate::{
            extractor::Extractor,
            generic::{Func, Tuple},
            input::Input,
        },
        futures01::{Future, Poll},
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
        type Future = MapFuture<E::Future, F>;

        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            MapFuture {
                future: self.extractor.extract(input),
                f: self.f.clone(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct MapFuture<Fut, F> {
        future: Fut,
        f: F,
    }

    impl<Fut, F> Future for MapFuture<Fut, F>
    where
        Fut: Future,
        Fut::Item: Tuple,
        F: Func<Fut::Item>,
    {
        type Item = (F::Out,);
        type Error = Fut::Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let args = futures01::try_ready!(self.future.poll());
            Ok((self.f.call(args),).into())
        }
    }
}
