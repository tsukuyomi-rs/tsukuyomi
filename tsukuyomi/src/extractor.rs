//! Definition of `Extractor` and its implementors.

pub mod body;
pub mod ext;
pub mod header;
pub mod local;
pub mod method;

pub use self::ext::ExtractorExt;

use {
    crate::{
        error::Error,
        future::TryFuture,
        generic::Tuple,
        input::Input,
        util::Never, //
    },
    serde::de::DeserializeOwned,
};

/// A trait abstracting the extraction of values from the incoming request.
pub trait Extractor {
    /// The type of output value extracted by `Extract`.
    type Output: Tuple;

    /// The error type that may be returned from `Extract`.
    type Error: Into<Error>;

    /// The type representing an asynchronous task to extract the value.
    type Extract: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Creates an instance of `Extract`.
    ///
    /// Note that the actual extraction process is started when the value
    /// of `Extract` is polled.
    fn extract(&self) -> Self::Extract;
}

impl<E> Extractor for Box<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Extract = E::Extract;

    #[inline]
    fn extract(&self) -> Self::Extract {
        (**self).extract()
    }
}

impl<E> Extractor for std::rc::Rc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Extract = E::Extract;

    #[inline]
    fn extract(&self) -> Self::Extract {
        (**self).extract()
    }
}

impl<E> Extractor for std::sync::Arc<E>
where
    E: Extractor,
{
    type Output = E::Output;
    type Error = E::Error;
    type Extract = E::Extract;

    #[inline]
    fn extract(&self) -> Self::Extract {
        (**self).extract()
    }
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Extract = self::unit::Unit;

    #[inline]
    fn extract(&self) -> Self::Extract {
        self::unit::Unit(())
    }
}

mod unit {
    use super::*;

    #[allow(missing_debug_implementations)]
    pub struct Unit(pub(super) ());

    impl TryFuture for Unit {
        type Ok = ();
        type Error = crate::util::Never;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> crate::future::Poll<Self::Ok, Self::Error> {
            Ok(().into())
        }
    }
}

// ==== primitives ====

/// Creates an `Extractor` from the provided function that returns a `TryFuture`.
pub fn extract<R>(
    f: impl Fn() -> R,
) -> impl Extractor<
    Output = R::Ok, //
    Error = R::Error,
    Extract = R,
>
where
    R: TryFuture,
    R::Ok: Tuple,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Extractor for Raw<F>
    where
        F: Fn() -> R,
        R: TryFuture,
        R::Ok: Tuple,
    {
        type Output = R::Ok;
        type Error = R::Error;
        type Extract = R;

        #[inline]
        fn extract(&self) -> Self::Extract {
            (self.0)()
        }
    }

    Raw(f)
}

/// Creates an `Extractor` from a *synchronous* function.
pub fn ready<F, T, E>(
    f: F,
) -> impl Extractor<
    Output = T, //
    Error = E,
    Extract = self::ready::Ready<F>, // private
>
where
    F: Fn(&mut Input<'_>) -> Result<T, E> + Clone,
    T: Tuple,
    E: Into<Error>,
{
    self::extract(move || self::ready::Ready(f.clone()))
}

mod ready {
    use crate::{
        error::Error,
        future::{Poll, TryFuture},
        generic::Tuple,
        input::Input,
    };

    #[allow(missing_debug_implementations)]
    pub struct Ready<F>(pub(super) F);

    impl<F, T, E> TryFuture for Ready<F>
    where
        F: Fn(&mut Input<'_>) -> Result<T, E>,
        T: Tuple,
        E: Into<Error>,
    {
        type Ok = T;
        type Error = E;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            (self.0)(input).map(Into::into)
        }
    }
}

/// Creates an `Extractor` that just clones and returns the provided value.
pub fn value<T>(
    value: T,
) -> impl Extractor<
    Output = (T,),
    Error = Never,
    Extract = self::value::Value<T>, // private
>
where
    T: Clone,
{
    self::extract(move || self::value::Value(Some(value.clone())))
}

mod value {
    use super::*;

    #[allow(missing_debug_implementations)]
    pub struct Value<T>(pub(super) Option<T>);

    impl<T> TryFuture for Value<T> {
        type Ok = (T,);
        type Error = crate::util::Never;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> crate::future::Poll<Self::Ok, Self::Error> {
            Ok((self.0.take().expect("the future has already been polled"),).into())
        }
    }
}

/// Creates an `Extractor` that returns the value of request method.
pub fn method() -> impl Extractor<
    Output = (http::Method,), //
    Error = Never,
    Extract = impl TryFuture<Ok = (http::Method,), Error = Never> + Send + 'static,
> {
    self::ready(|input| Ok((input.request.method().clone(),)))
}

/// Creates an `Extractor` that returns the value of request URI.
pub fn uri() -> impl Extractor<
    Output = (http::Uri,), //
    Error = Never,
    Extract = impl TryFuture<Ok = (http::Uri,), Error = Never> + Send + 'static,
> {
    self::ready(|input| Ok((input.request.uri().clone(),)))
}

/// Creates an `Extractor` that returns the value of the request version.
pub fn version() -> impl Extractor<
    Output = (http::Version,), //
    Error = Never,
    Extract = impl TryFuture<Ok = (http::Version,), Error = Never> + Send + 'static,
> {
    self::ready(|input| Ok((input.request.version(),)))
}

/// Creates an `Extractor` that parses the value of query string to `T`.
pub fn query<T>() -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: DeserializeOwned,
{
    self::ready(move |input| {
        let query_str = input
            .request
            .uri()
            .query()
            .ok_or_else(|| crate::error::bad_request("missing query"))?;
        serde_urlencoded::from_str(query_str) //
            .map(|x| (x,))
            .map_err(crate::error::bad_request)
    })
}

/// Creates an `Extractor` that returns the value of extension of the specified type.
pub fn extension<T>() -> impl Extractor<
    Output = (T,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (T,), Error = Error> + Send + 'static,
>
where
    T: Clone + Send + Sync + 'static,
{
    self::ready(|input| {
        input
            .request
            .extensions()
            .get()
            .cloned()
            .ok_or_else(|| crate::error::internal_server_error("missing extension"))
    })
}
