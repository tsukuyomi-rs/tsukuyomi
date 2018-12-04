//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod builder;
mod generic;

pub mod body;
pub mod extension;
pub mod header;
pub mod local;
pub mod param;
pub mod query;
pub mod verb;

pub use self::builder::Builder;
pub(crate) use self::generic::{Combine, Func, Tuple};

use {
    crate::{common::Never, error::Error, input::Input},
    futures::{Future, IntoFuture, Poll},
    std::marker::PhantomData,
};

/// A type that represents the value of a `Future` never constructed.
#[doc(hidden)]
#[derive(Debug)]
pub struct Placeholder<T, E> {
    never: Never,
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

    fn into_builder(self) -> Builder<Self>
    where
        Self: Sized,
    {
        Builder::new(self)
    }
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
    #[derive(Debug, Copy, Clone)]
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
    F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
    E: Into<Error> + 'static,
{
    self::raw(
        move |input| -> Result<ExtractStatus<(), self::Placeholder<_, _>>, E> {
            f(input)?;
            Ok(ExtractStatus::Ready(()))
        },
    )
}

pub fn ready<F, T, E>(f: F) -> impl Extractor<Output = (T,), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
    T: 'static,
    E: Into<Error> + 'static,
{
    self::raw(
        move |input| -> Result<ExtractStatus<_, self::Placeholder<_, _>>, _> {
            f(input).map(|x| ExtractStatus::Ready((x,)))
        },
    )
}

pub fn lazy<F, R>(f: F) -> impl Extractor<Output = (R::Item,), Error = R::Error>
where
    F: Fn(&mut Input<'_>) -> R + Send + Sync + 'static,
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: 'static,
    R::Error: Into<Error> + 'static,
{
    self::raw(move |input| {
        Ok(ExtractStatus::Pending(
            f(input).into_future().map(|output| (output,)),
        ))
    })
}

pub fn value<T>(value: T) -> impl Extractor<Output = (T,), Error = Never>
where
    T: Clone + Send + Sync + 'static,
{
    self::raw(move |_| -> Result<ExtractStatus<_, Placeholder<_, _>>, _> {
        Ok(ExtractStatus::Ready((value.clone(),)))
    })
}

pub fn method() -> impl Extractor<Output = (http::Method,), Error = Never> {
    self::ready(|input| Ok(input.request.method().clone()))
}

pub fn uri() -> impl Extractor<Output = (http::Uri,), Error = Never> {
    self::ready(|input| Ok(input.request.uri().clone()))
}

pub fn version() -> impl Extractor<Output = (http::Version,), Error = Never> {
    self::ready(|input| Ok(input.request.version()))
}
