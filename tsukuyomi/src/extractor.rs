//! API for extracting the incoming request information from the request context.

#![allow(missing_docs)]

pub mod body;
pub mod ext;
pub mod extension;
pub mod header;
pub mod local;
pub mod method;
pub mod query;

pub use self::ext::ExtractorExt;

use {
    crate::{
        core::Never, //
        error::Error,
        generic::Tuple,
        input::Input,
    },
    futures01::{Future, IntoFuture},
};

/// A trait abstracting the extraction of values from `Input`.
pub trait Extractor {
    /// The type of output value from this extractor.
    type Output: Tuple;

    type Error: Into<Error>;

    /// The type representing asyncrhonous computations performed during extraction.
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    /// Performs extraction from the specified `Input`.
    fn extract(&self, input: &mut Input<'_>) -> Self::Future;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Self::Future {
        (**self).extract(input)
    }
}

impl<E> Extractor for std::rc::Rc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Future = E::Future;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Self::Future {
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
    fn extract(&self, input: &mut Input<'_>) -> Self::Future {
        (**self).extract(input)
    }
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Future = futures01::future::FutureResult<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Self::Future {
        futures01::future::ok(())
    }
}

// ==== primitives ====

pub fn raw<F, R>(f: F) -> impl Extractor<Output = R::Item, Error = R::Error, Future = R::Future>
where
    F: Fn(&mut Input<'_>) -> R,
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: Tuple,
    R::Error: Into<Error>,
{
    #[derive(Debug, Copy, Clone)]
    struct Raw<F>(F);

    #[allow(clippy::type_complexity)]
    impl<F, R> Extractor for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> R,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Item: Tuple,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> Self::Future {
            (self.0)(input).into_future()
        }
    }

    Raw(f)
}

pub fn guard<F, E>(
    f: F,
) -> impl Extractor<Output = (), Error = E, Future = futures01::future::FutureResult<(), E>>
where
    F: Fn(&mut Input<'_>) -> Result<(), E>,
    E: Into<Error> + Send + 'static,
{
    self::raw(move |input| futures01::future::result(f(input)))
}

pub fn ready<F, T, E>(
    f: F,
) -> impl Extractor<Output = (T,), Error = E, Future = futures01::future::FutureResult<(T,), E>>
where
    F: Fn(&mut Input<'_>) -> Result<T, E>,
    T: Send + 'static,
    E: Into<Error> + Send + 'static,
{
    self::raw(move |input| futures01::future::result(f(input).map(|x| (x,))))
}

pub fn value<T>(
    value: T,
) -> impl Extractor<Output = (T,), Error = Never, Future = futures01::future::FutureResult<(T,), Never>>
where
    T: Clone + Send + 'static,
{
    self::ready(move |_| Ok(value.clone()))
}

pub fn method() -> impl Extractor<
    Output = (http::Method,),
    Error = Never,
    Future = futures01::future::FutureResult<(http::Method,), Never>,
> {
    self::ready(|input| Ok(input.request.method().clone()))
}

pub fn uri() -> impl Extractor<
    Output = (http::Uri,),
    Error = Never,
    Future = futures01::future::FutureResult<(http::Uri,), Never>,
> {
    self::ready(|input| Ok(input.request.uri().clone()))
}

pub fn version() -> impl Extractor<
    Output = (http::Version,),
    Error = Never,
    Future = futures01::future::FutureResult<(http::Version,), Never>,
> {
    self::ready(|input| Ok(input.request.version()))
}
