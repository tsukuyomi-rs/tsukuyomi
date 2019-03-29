use {
    crate::{
        error::Error,
        extractor::Extractor,
        future::{Poll, TryFuture},
        generic::Tuple,
        input::param::Params,
        input::Input,
    },
    std::marker::PhantomData,
};

#[doc(hidden)]
pub use tsukuyomi_macros::path_impl;

pub trait Path {
    type Output: Tuple;

    fn as_str(&self) -> &str;
    fn extract(params: Option<&Params<'_>>) -> Result<Self::Output, Error>;
}

impl Path for &'static str {
    type Output = ();

    #[inline]
    fn as_str(&self) -> &str {
        self
    }

    #[inline]
    fn extract(_: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        Ok(())
    }
}

impl Path for String {
    type Output = ();

    #[inline]
    fn as_str(&self) -> &str {
        self.as_str()
    }

    #[inline]
    fn extract(_: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        Ok(())
    }
}

/// A macro for generating the code that creates a [`Path`] from the provided tokens.
///
/// [`Path`]: ./app/config/route/struct.Path.html
#[macro_export]
macro_rules! path {
    ($path:expr) => {{
        use $crate::app::path::internal as __path_internal;
        enum __Dummy {}
        impl __Dummy {
            $crate::app::path::path_impl!(__path_internal, $path);
        }
        __Dummy::call()
    }};
}

#[doc(hidden)]
pub mod internal {
    pub use {
        super::Path,
        crate::{
            error::Error,
            input::param::{FromPercentEncoded, Params, PercentEncoded},
        },
    };
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct PathExtractor<P: Path + ?Sized> {
    _marker: PhantomData<P>,
}

impl<P> PathExtractor<P>
where
    P: Path + ?Sized,
{
    pub(crate) fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<P> Extractor for PathExtractor<P>
where
    P: Path + ?Sized,
{
    type Output = P::Output;
    type Error = Error;
    type Extract = PathExtract<P>;

    fn extract(&self) -> Self::Extract {
        PathExtract {
            _marker: PhantomData,
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct PathExtract<P: Path + ?Sized> {
    _marker: PhantomData<P>,
}

impl<P> TryFuture for PathExtract<P>
where
    P: Path + ?Sized,
{
    type Ok = P::Output;
    type Error = Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        P::extract(input.params.as_ref()).map(Into::into)
    }
}
