use {
    super::Extractor,
    crate::{
        generic::{Combine, Func},
        util::Chain, //
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
            error::Error,
            future::{Async, MaybeDone, Poll, TryFuture},
            generic::{Combine, Tuple},
            input::Input,
            util::Chain,
        },
    };

    impl<L, R> Extractor for Chain<L, R>
    where
        L: Extractor,
        R: Extractor,
        L::Output: Combine<R::Output>,
    {
        type Output = <L::Output as Combine<R::Output>>::Out;
        type Error = Error;
        type Extract = ChainFuture<L::Extract, R::Extract>;

        fn extract(&self) -> Self::Extract {
            ChainFuture {
                left: MaybeDone::Pending(self.left.extract()),
                right: MaybeDone::Pending(self.right.extract()),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct ChainFuture<L: TryFuture, R: TryFuture> {
        left: MaybeDone<L>,
        right: MaybeDone<R>,
    }

    impl<L: TryFuture, R: TryFuture> ChainFuture<L, R> {
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<(), Error> {
            futures01::try_ready!(self.left.poll_ready(input).map_err(Into::into));
            futures01::try_ready!(self.right.poll_ready(input).map_err(Into::into));
            Ok(Async::Ready(()))
        }
    }

    impl<L: TryFuture, R: TryFuture> TryFuture for ChainFuture<L, R>
    where
        L::Ok: Combine<R::Ok>,
        R::Ok: Tuple,
    {
        type Ok = <L::Ok as Combine<R::Ok>>::Out;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self.poll_ready(input) {
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
    use crate::{
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        input::Input,
        util::Never,
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
        type Extract = OptionalFuture<E::Extract>;

        fn extract(&self) -> Self::Extract {
            OptionalFuture {
                0: self.extractor.extract(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct OptionalFuture<Fut>(Fut);

    impl<Fut, T> TryFuture for OptionalFuture<Fut>
    where
        Fut: TryFuture<Ok = (T,)>,
    {
        type Ok = (Option<T>,);
        type Error = Never;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self.0.poll_ready(input) {
                Ok(Async::Ready((ok,))) => Ok(Async::Ready((Some(ok),))),
                Err(..) => Ok(Async::Ready((None,))),
                Ok(Async::NotReady) => Ok(Async::NotReady),
            }
        }
    }
}

mod either_or {
    use crate::{
        error::Error,
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        generic::Tuple,
        input::Input,
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
        type Extract = EitherOrFuture<L::Extract, R::Extract>;

        fn extract(&self) -> Self::Extract {
            EitherOrFuture {
                left: Some(self.left.extract()),
                right: Some(self.right.extract()),
            }
        }
    }

    #[derive(Debug)]
    pub struct EitherOrFuture<L, R> {
        left: Option<L>,
        right: Option<R>,
    }

    impl<L, R, T> TryFuture for EitherOrFuture<L, R>
    where
        L: TryFuture<Ok = T>,
        R: TryFuture<Ok = T>,
    {
        type Ok = T;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                match (&mut self.left, &mut self.right) {
                    (Some(left), Some(right)) => match left.poll_ready(input) {
                        Ok(Async::NotReady) => match right.poll_ready(input) {
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
                    (Some(left), None) => return left.poll_ready(input).map_err(Into::into),
                    (None, Some(right)) => return right.poll_ready(input).map_err(Into::into),
                    (None, None) => unreachable!(),
                }
            }
        }
    }
}

mod map {
    use crate::{
        extractor::Extractor,
        future::{Poll, TryFuture},
        generic::{Func, Tuple},
        input::Input,
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
        type Extract = MapFuture<E::Extract, F>;

        fn extract(&self) -> Self::Extract {
            MapFuture {
                future: self.extractor.extract(),
                f: self.f.clone(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct MapFuture<Fut, F> {
        future: Fut,
        f: F,
    }

    impl<Fut, F> TryFuture for MapFuture<Fut, F>
    where
        Fut: TryFuture,
        Fut::Ok: Tuple,
        F: Func<Fut::Ok>,
    {
        type Ok = (F::Out,);
        type Error = Fut::Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let args = futures01::try_ready!(self.future.poll_ready(input));
            Ok((self.f.call(args),).into())
        }
    }
}
