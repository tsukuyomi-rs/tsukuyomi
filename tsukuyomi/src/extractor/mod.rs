//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod and;
mod and_then;
mod generic;
mod map;
mod optional;
mod or;

pub mod body;
pub mod extension;
pub mod header;
pub mod local;
pub mod param;
pub mod query;
pub mod state;
pub mod verb;

pub use self::and::And;
pub use self::and_then::AndThen;
pub(crate) use self::generic::{Combine, Func, Tuple};
pub use self::map::Map;
pub use self::optional::Optional;
pub use self::or::Or;

// ==== impl ====

use futures::future;
use futures::{Async, Future, IntoFuture, Poll};

use crate::error::{Error, Never};
use crate::input::Input;

pub trait Extractor: Send + Sync + 'static {
    type Output: Tuple;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error>;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
        (**self).extract(input)
    }
}

impl<E> Extractor for std::sync::Arc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
        (**self).extract(input)
    }
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Future = Matched;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
        Ok(Matched(()))
    }
}

#[derive(Debug, Default)]
#[must_use = "futures do nothing unless polled"]
pub struct Matched(());

impl Future for Matched {
    type Item = ();
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(()))
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Extracted<T>(Option<T>);

impl<T> From<T> for Extracted<T> {
    fn from(value: T) -> Self {
        Extracted(Some(value))
    }
}

impl<T> Future for Extracted<T> {
    type Item = (T,);
    type Error = Never;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready((self
            .0
            .take()
            .expect("This future has already polled."),)))
    }
}

// ==== ExtractorExt ====

#[inline]
pub(crate) fn assert_impl_extractor<E>(extractor: E) -> E
where
    E: Extractor,
{
    extractor
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ExtractorExt: Extractor + Sized {
    fn optional<T>(self) -> Optional<Self>
    where
        Self: Extractor<Output = (T,)>,
    {
        assert_impl_extractor(Optional(self))
    }

    fn and<E>(self, other: E) -> And<Self, E>
    where
        E: Extractor,
        Self::Output: Combine<E::Output> + Send + 'static,
        E::Output: Send + 'static,
    {
        assert_impl_extractor(And {
            left: self,
            right: other,
        })
    }

    fn or<E>(self, other: E) -> Or<Self, E>
    where
        E: Extractor<Output = Self::Output>,
    {
        assert_impl_extractor(Or {
            left: self,
            right: other,
        })
    }

    fn map<F>(self, f: F) -> Map<Self, F>
    where
        F: Func<Self::Output> + Clone + Send + Sync + 'static,
    {
        assert_impl_extractor(Map { extractor: self, f })
    }

    fn and_then<F, R>(self, f: F) -> AndThen<Self, F>
    where
        F: Func<Self::Output, Out = R> + Clone + Send + Sync + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        assert_impl_extractor(AndThen { extractor: self, f })
    }
}

impl<E: Extractor> ExtractorExt for E {}

// ==== primitives ====

pub fn unit() -> impl Extractor<Output = (), Error = Never> {
    ()
}

pub fn validate<F, E>(f: F) -> impl Extractor<Output = (), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
    E: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Validate<F>(F);

    impl<F, E> Extractor for Validate<F>
    where
        F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
        E: Into<Error> + 'static,
    {
        type Output = ();
        type Error = E;
        type Future = ValidateFuture<E>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
            (self.0)(input).map(|()| ValidateFuture(std::marker::PhantomData))
        }
    }

    #[allow(missing_debug_implementations)]
    struct ValidateFuture<E>(std::marker::PhantomData<fn() -> E>);

    impl<E> Future for ValidateFuture<E> {
        type Item = ();
        type Error = E;

        #[inline]
        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            Ok(Async::Ready(()))
        }
    }

    Validate(f)
}

pub fn ready<F, T, E>(f: F) -> impl Extractor<Output = (T,), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
    T: Send + 'static,
    E: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Ready<F>(F);

    impl<F, T, E> Extractor for Ready<F>
    where
        F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
        T: Send + 'static,
        E: Into<Error> + 'static,
    {
        type Output = (T,);
        type Error = E;
        type Future = ReadyFuture<T, E>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
            (self.0)(input).map(|x| ReadyFuture(Some(x), std::marker::PhantomData))
        }
    }

    #[allow(missing_debug_implementations)]
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    struct ReadyFuture<T, E>(Option<T>, std::marker::PhantomData<fn() -> E>);

    impl<T, E> Future for ReadyFuture<T, E> {
        type Item = (T,);
        type Error = E;

        #[inline]
        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            Ok(Async::Ready((self
                .0
                .take()
                .expect("This future has already polled"),)))
        }
    }

    Ready(f)
}

pub fn lazy<F, R>(f: F) -> impl Extractor<Output = (R::Item,), Error = R::Error>
where
    F: Fn(&mut Input<'_>) -> Result<R, R::Error> + Send + Sync + 'static,
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: 'static,
    R::Error: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Lazy<F>(F);

    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    impl<F, R> Extractor for Lazy<F>
    where
        F: Fn(&mut Input<'_>) -> Result<R, R::Error> + Send + Sync + 'static,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Item: 'static,
        R::Error: Into<Error> + 'static,
    {
        type Output = (R::Item,);
        type Error = R::Error;
        type Future = futures::future::Map<R::Future, fn(R::Item) -> (R::Item,)>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
            (self.0)(input).map(|future| {
                future
                    .into_future()
                    .map((|output| (output,)) as fn(R::Item) -> (R::Item,))
            })
        }
    }

    Lazy(f)
}

pub fn value<T>(value: T) -> impl Extractor<Output = (T,), Error = Never>
where
    T: Clone + Send + Sync + 'static,
{
    #[allow(missing_debug_implementations)]
    struct ValueExtractor<T>(T);

    impl<T> Extractor for ValueExtractor<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        type Output = (T,);
        type Error = Never;
        type Future = Extracted<T>;

        #[inline]
        fn extract(&self, _: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
            Ok(Extracted::from(self.0.clone()))
        }
    }

    ValueExtractor(value)
}

pub fn method() -> impl Extractor<Output = (http::Method,), Error = Never> {
    self::ready(|input| Ok(input.method().clone()))
}

pub fn uri() -> impl Extractor<Output = (http::Uri,), Error = Never> {
    self::ready(|input| Ok(input.uri().clone()))
}

pub fn version() -> impl Extractor<Output = (http::Version,), Error = Never> {
    self::ready(|input| Ok(input.version()))
}
