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

use std::marker::PhantomData;

use futures::future;
use futures::{Async, Future, IntoFuture, Poll};

use crate::error::{Error, Never};
use crate::input::Input;
use crate::output::Output;

/// A type that represents the value of a `Future` never constructed.
#[derive(Debug)]
pub struct Placeholder<T, E> {
    never: crate::error::Never,
    _marker: PhantomData<fn() -> (T, E)>,
}

impl<T, E> Future for Placeholder<T, E> {
    type Item = T;
    type Error = E;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.never {}
    }
}

/// An enum representing the result of `Extractor`.
#[derive(Debug)]
pub enum ExtractStatus<T, Fut> {
    /// The value of `T` is immediately available.
    Ready(T),

    /// The value has not been available yet.
    Pending(Fut),

    /// Cancel the subsequent extraction and return the specified output to the client.
    Canceled(Output),
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<T, Fut> ExtractStatus<T, Fut> {
    pub fn map<U, V>(
        self,
        f: impl FnOnce(T) -> U,
        g: impl FnOnce(Fut) -> V,
    ) -> ExtractStatus<U, V> {
        match self {
            ExtractStatus::Ready(t) => ExtractStatus::Ready(f(t)),
            ExtractStatus::Pending(fut) => ExtractStatus::Pending(g(fut)),
            ExtractStatus::Canceled(out) => ExtractStatus::Canceled(out),
        }
    }

    pub fn map_ready<U>(self, f: impl FnOnce(T) -> U) -> ExtractStatus<U, Fut> {
        self.map(f, |fut| fut)
    }

    pub fn map_pending<U>(self, f: impl FnOnce(Fut) -> U) -> ExtractStatus<T, U> {
        self.map(|t| t, f)
    }
}

/// A type alias representing the return type of `Extractor::extract`.
pub type Extract<E> = Result<
    ExtractStatus<<E as Extractor>::Output, <E as Extractor>::Future>,
    <E as Extractor>::Error,
>;

/// A trait abstracting the extraction of values from `Input`.
pub trait Extractor: Send + Sync + 'static {
    /// The type of output value from this extractor.
    type Output: Tuple;

    /// The error type which will be returned from this extractor.
    type Error: Into<Error>;

    /// The type representing asyncrhonous computations performed during extraction.
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    /// Performs extraction from the specified `Input`.
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self>;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
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
    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        (**self).extract(input)
    }
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Future = Placeholder<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Extract<Self> {
        Ok(ExtractStatus::Ready(()))
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

pub fn raw<F, R>(f: F) -> impl Extractor<Output = R::Item, Error = R::Error>
where
    F: Fn(&mut Input<'_>) -> Result<ExtractStatus<R::Item, R>, R::Error> + Send + Sync + 'static,
    R: Future + Send + 'static,
    R::Item: Tuple + 'static,
    R::Error: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    impl<F, R> Extractor for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> Result<ExtractStatus<R::Item, R>, R::Error>
            + Send
            + Sync
            + 'static,
        R: Future + Send + 'static,
        R::Item: Tuple + 'static,
        R::Error: Into<Error> + 'static,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = R;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
            (self.0)(input)
        }
    }

    Raw(f)
}

pub fn guard<F, E>(f: F) -> impl Extractor<Output = (), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<Option<Output>, E> + Send + Sync + 'static,
    E: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Guard<F>(F);

    impl<F, E> Extractor for Guard<F>
    where
        F: Fn(&mut Input<'_>) -> Result<Option<Output>, E> + Send + Sync + 'static,
        E: Into<Error> + 'static,
    {
        type Output = ();
        type Error = E;
        type Future = Placeholder<Self::Output, Self::Error>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
            match (self.0)(input)? {
                Some(output) => Ok(ExtractStatus::Canceled(output)),
                None => Ok(ExtractStatus::Ready(())),
            }
        }
    }

    Guard(f)
}

pub fn ready<F, T, E>(f: F) -> impl Extractor<Output = (T,), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
    T: 'static,
    E: Into<Error> + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Ready<F>(F);

    impl<F, T, E> Extractor for Ready<F>
    where
        F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
        T: 'static,
        E: Into<Error> + 'static,
    {
        type Output = (T,);
        type Error = E;
        type Future = Placeholder<Self::Output, Self::Error>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
            (self.0)(input).map(|x| ExtractStatus::Ready((x,)))
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
        fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
            (self.0)(input).map(|future| {
                ExtractStatus::Pending(
                    future
                        .into_future()
                        .map((|output| (output,)) as fn(R::Item) -> (R::Item,)),
                )
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
        type Future = Placeholder<Self::Output, Self::Error>;

        #[inline]
        fn extract(&self, _: &mut Input<'_>) -> Extract<Self> {
            Ok(ExtractStatus::Ready((self.0.clone(),)))
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
