//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod builder;
mod generic;

pub mod body;
pub mod extension;
pub mod header;
pub mod local;
pub mod query;
pub mod verb;

pub use self::builder::Builder;
pub(crate) use self::generic::{Combine, Func, Tuple};

use {
    crate::{
        common::{MaybeFuture, Never, NeverFuture},
        error::Error,
        input::Input,
    },
    futures::{Future, IntoFuture},
};

/// A type alias representing the return type of `Extractor::extract`.
pub type Extract<E> = MaybeFuture<<E as Extractor>::Future>;

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
    type Future = NeverFuture<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Extract<Self> {
        MaybeFuture::ok(())
    }
}

// ==== primitives ====

pub fn unit() -> impl Extractor<Output = (), Error = Never> {
    ()
}

pub fn raw<F, R>(f: F) -> impl Extractor<Output = R::Item, Error = R::Error>
where
    F: Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
    R: Future + Send + 'static,
    R::Item: Tuple,
    R::Error: Into<Error>,
{
    #[derive(Debug, Copy, Clone)]
    struct Raw<F>(F);

    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    impl<F, R> Extractor for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
        R: Future + Send + 'static,
        R::Item: Tuple,
        R::Error: Into<Error>,
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
    self::raw(move |input| MaybeFuture::Ready::<NeverFuture<_, _>>(f(input)))
}

pub fn ready<F, T, E>(f: F) -> impl Extractor<Output = (T,), Error = E>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
    T: 'static,
    E: Into<Error> + 'static,
{
    self::raw(move |input| MaybeFuture::Ready::<NeverFuture<_, _>>(f(input).map(|x| (x,))))
}

pub fn lazy<F, R>(f: F) -> impl Extractor<Output = (R::Item,), Error = R::Error>
where
    F: Fn(&mut Input<'_>) -> R + Send + Sync + 'static,
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: 'static,
    R::Error: Into<Error> + 'static,
{
    self::raw(move |input| MaybeFuture::from(f(input).into_future().map(|output| (output,))))
}

pub fn value<T>(value: T) -> impl Extractor<Output = (T,), Error = Never>
where
    T: Clone + Send + Sync + 'static,
{
    self::raw(move |_| MaybeFuture::<NeverFuture<_, _>>::ok((value.clone(),)))
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
