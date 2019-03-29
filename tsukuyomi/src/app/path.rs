use {
    crate::{error::Error, generic::Tuple, input::param::Params},
    std::marker::PhantomData,
};

#[doc(hidden)]
pub use tsukuyomi_macros::path_impl;

pub trait PathExtractor {
    type Output: Tuple;

    fn extract(params: Option<&Params<'_>>) -> Result<Self::Output, Error>;
}

impl PathExtractor for () {
    type Output = ();

    #[inline]
    fn extract(_: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        Ok(())
    }
}

pub trait IntoPath {
    type Output: Tuple;
    type Extractor: PathExtractor<Output = Self::Output>;

    fn into_path(self) -> Path<Self::Extractor>;
}

impl IntoPath for &'static str {
    type Output = ();
    type Extractor = ();

    fn into_path(self) -> Path<Self::Extractor> {
        Path::new(self)
    }
}

impl<T> IntoPath for Path<T>
where
    T: PathExtractor,
{
    type Output = T::Output;
    type Extractor = T;

    fn into_path(self) -> Path<Self::Extractor> {
        self
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
        super::{Path, PathExtractor},
        crate::{
            error::Error,
            input::param::{FromPercentEncoded, Params, PercentEncoded},
        },
    };
}

#[derive(Debug)]
pub struct Path<E: PathExtractor = ()> {
    path: &'static str,
    _marker: PhantomData<E>,
}

impl<E> Path<E>
where
    E: PathExtractor,
{
    /// Creates a new `Path` with the specified path and extractor.
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            _marker: PhantomData,
        }
    }

    pub(crate) fn uri_str(&self) -> &'static str {
        self.path
    }
}
