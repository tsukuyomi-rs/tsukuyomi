//! A set of extensions for `Extractor`s.

use {
    super::Extractor,
    crate::{
        error::Error,
        generic::{Combine, Func},
        util::Chain, //
    },
};

pub use self::{
    fallible::Fallible, //
    map::Map,
    map_err::MapErr,
    optional::Optional,
    or::Or,
};

/// A set of extension methods for composing/formatting `Extractor`s.
pub trait ExtractorExt: Extractor + Sized {
    fn optional<T>(self) -> Optional<Self, T>
    where
        Self: Extractor<Output = (T,)>,
    {
        Optional {
            extractor: self,
            _marker: std::marker::PhantomData,
        }
    }

    fn fallible<T>(self) -> Fallible<Self, T>
    where
        Self: Extractor<Output = (T,)>,
    {
        Fallible {
            extractor: self,
            _marker: std::marker::PhantomData,
        }
    }

    fn and<E>(self, other: E) -> Chain<Self, E>
    where
        Self: Sized,
        E: Extractor,
        Self::Output: Combine<E::Output>,
    {
        Chain::new(self, other)
    }

    fn or<E>(self, other: E) -> Or<Self, E>
    where
        E: Extractor<Output = Self::Output>,
    {
        Or {
            left: self,
            right: other,
        }
    }

    fn map<F>(self, f: F) -> Map<Self, F>
    where
        F: Func<Self::Output> + Clone,
    {
        Map { extractor: self, f }
    }

    fn map_err<F, U>(self, f: F) -> MapErr<Self, F>
    where
        F: Fn(Self::Error) -> U + Clone,
        U: Into<Error>,
    {
        MapErr { extractor: self, f }
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
                extract: self.extractor.extract(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct OptionalFuture<E> {
        extract: E,
    }

    impl<E, T> TryFuture for OptionalFuture<E>
    where
        E: TryFuture<Ok = (T,)>,
    {
        type Ok = (Option<T>,);
        type Error = Never;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self.extract.poll_ready(input) {
                Ok(Async::Ready((ok,))) => Ok(Async::Ready((Some(ok),))),
                Err(..) => Ok(Async::Ready((None,))),
                Ok(Async::NotReady) => Ok(Async::NotReady),
            }
        }
    }
}

mod fallible {
    use crate::{
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        input::Input,
        util::Never,
    };

    #[derive(Debug)]
    pub struct Fallible<E, T> {
        pub(super) extractor: E,
        pub(super) _marker: std::marker::PhantomData<fn() -> T>,
    }

    impl<E, T> Extractor for Fallible<E, T>
    where
        E: Extractor<Output = (T,)>,
    {
        type Output = (Result<T, E::Error>,);
        type Error = Never;
        type Extract = FallibleFuture<E::Extract>;

        fn extract(&self) -> Self::Extract {
            FallibleFuture {
                extract: self.extractor.extract(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct FallibleFuture<E> {
        extract: E,
    }

    impl<E, T> TryFuture for FallibleFuture<E>
    where
        E: TryFuture<Ok = (T,)>,
    {
        type Ok = (Result<T, E::Error>,);
        type Error = Never;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self.extract.poll_ready(input) {
                Ok(Async::Ready((ok,))) => Ok(Async::Ready((Ok(ok),))),
                Err(err) => Ok(Async::Ready((Err(err),))),
                Ok(Async::NotReady) => Ok(Async::NotReady),
            }
        }
    }
}

mod or {
    use crate::{
        error::Error,
        extractor::Extractor,
        future::{Async, Poll, TryFuture},
        generic::Tuple,
        input::Input,
    };

    #[derive(Debug)]
    pub struct Or<L, R> {
        pub(super) left: L,
        pub(super) right: R,
    }

    impl<L, R, T: Tuple> Extractor for Or<L, R>
    where
        L: Extractor<Output = T>,
        R: Extractor<Output = T>,
    {
        type Output = T;
        type Error = Error;
        type Extract = OrFuture<L::Extract, R::Extract>;

        fn extract(&self) -> Self::Extract {
            OrFuture {
                left: Some(self.left.extract()),
                right: Some(self.right.extract()),
            }
        }
    }

    #[derive(Debug)]
    pub struct OrFuture<L, R> {
        left: Option<L>,
        right: Option<R>,
    }

    impl<L, R, T> TryFuture for OrFuture<L, R>
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

mod map_err {
    use crate::{
        error::Error,
        extractor::Extractor,
        future::{Poll, TryFuture},
        generic::Tuple,
        input::Input,
    };

    #[derive(Debug)]
    pub struct MapErr<E, F> {
        pub(super) extractor: E,
        pub(super) f: F,
    }

    impl<E, F, U> Extractor for MapErr<E, F>
    where
        E: Extractor,
        E::Error: 'static,
        F: Fn(E::Error) -> U + Clone + Send + 'static,
        U: Into<Error>,
    {
        type Output = E::Output;
        type Error = U;
        type Extract = MapErrFuture<E::Extract, F>;

        fn extract(&self) -> Self::Extract {
            MapErrFuture {
                future: self.extractor.extract(),
                f: self.f.clone(),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct MapErrFuture<Fut, F> {
        future: Fut,
        f: F,
    }

    impl<Fut, F, U> TryFuture for MapErrFuture<Fut, F>
    where
        Fut: TryFuture,
        Fut::Ok: Tuple,
        F: Fn(Fut::Error) -> U,
        U: Into<Error>,
    {
        type Ok = Fut::Ok;
        type Error = U;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            self.future.poll_ready(input).map_err(|err| (self.f)(err))
        }
    }
}
