//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod chain;
mod ext;

pub mod body;
pub mod extension;
pub mod header;
pub mod local;
pub mod query;
pub mod verb;

pub use self::ext::ExtractorExt;

use crate::{
    common::Never,
    error::Error,
    future::{Future, MaybeFuture, NeverFuture},
    generic::Tuple,
    input::Input,
};

/// A trait abstracting the extraction of values from `Input`.
pub trait Extractor: Send + Sync + 'static {
    /// The type of output value from this extractor.
    type Output: Tuple;

    /// The type representing asyncrhonous computations performed during extraction.
    type Future: Future<Output = Self::Output> + Send + 'static;

    /// Performs extraction from the specified `Input`.
    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future>;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        (**self).extract(input)
    }
}

impl<E> Extractor for std::sync::Arc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        (**self).extract(input)
    }
}

impl Extractor for () {
    type Output = ();
    type Future = NeverFuture<Self::Output, Never>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        MaybeFuture::ok(())
    }
}

// ==== primitives ====

pub fn raw<F, R>(f: F) -> impl Extractor<Output = R::Output>
where
    F: Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
    R: Future + Send + 'static,
    R::Output: Tuple,
{
    #[derive(Debug, Copy, Clone)]
    struct Raw<F>(F);

    #[allow(clippy::type_complexity)]
    impl<F, R> Extractor for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
        R: Future + Send + 'static,
        R::Output: Tuple,
    {
        type Output = R::Output;
        type Future = R;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            (self.0)(input)
        }
    }

    Raw(f)
}

pub fn guard<F, E>(f: F) -> impl Extractor<Output = ()>
where
    F: Fn(&mut Input<'_>) -> Result<(), E> + Send + Sync + 'static,
    E: Into<Error> + 'static,
{
    self::raw(move |input| MaybeFuture::Ready::<NeverFuture<_, _>>(f(input)))
}

pub fn ready<F, T, E>(f: F) -> impl Extractor<Output = (T,)>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Send + Sync + 'static,
    T: 'static,
    E: Into<Error> + 'static,
{
    self::raw(move |input| MaybeFuture::Ready::<NeverFuture<_, _>>(f(input).map(|x| (x,))))
}

pub fn value<T>(value: T) -> impl Extractor<Output = (T,)>
where
    T: Clone + Send + Sync + 'static,
{
    self::raw(move |_| MaybeFuture::<NeverFuture<_, Never>>::ok((value.clone(),)))
}

pub fn method() -> impl Extractor<Output = (http::Method,)> {
    self::ready(|input| Ok::<_, Never>(input.request.method().clone()))
}

pub fn uri() -> impl Extractor<Output = (http::Uri,)> {
    self::ready(|input| Ok::<_, Never>(input.request.uri().clone()))
}

pub fn version() -> impl Extractor<Output = (http::Version,)> {
    self::ready(|input| Ok::<_, Never>(input.request.version()))
}
