use std::fmt;
use std::marker::PhantomData;

use crate::error::{Error, Never};
use crate::extractor::{Extract, Extractor};
use crate::input::local_map::LocalKey;
use crate::input::{Extension, Input, State};

pub trait HasExtractor: Sized {
    type Extractor: Extractor<Output = (Self,)>;
    fn extractor() -> Self::Extractor;
}

// ---- implementors ----

pub struct RequestExtractor<T>(fn(&mut Input<'_>) -> T);

impl<T> fmt::Debug for RequestExtractor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestExtractor").finish()
    }
}

impl<T: 'static> Extractor for RequestExtractor<T> {
    type Output = (T,);
    type Error = Never;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        Ok(Extract::Ready(((self.0)(input),)))
    }
}

pub fn method() -> <http::Method as HasExtractor>::Extractor {
    http::Method::extractor()
}

pub fn uri() -> <http::Uri as HasExtractor>::Extractor {
    http::Uri::extractor()
}

pub fn version() -> <http::Version as HasExtractor>::Extractor {
    http::Version::extractor()
}

impl HasExtractor for http::Method {
    type Extractor = RequestExtractor<Self>;

    #[inline]
    fn extractor() -> Self::Extractor {
        RequestExtractor(|input| input.method().clone())
    }
}

impl HasExtractor for http::Uri {
    type Extractor = RequestExtractor<Self>;

    #[inline]
    fn extractor() -> Self::Extractor {
        RequestExtractor(|input| input.uri().clone())
    }
}

impl HasExtractor for http::Version {
    type Extractor = RequestExtractor<Self>;

    #[inline]
    fn extractor() -> Self::Extractor {
        RequestExtractor(|input| input.version())
    }
}

// ==== Extension ====

pub fn extension<T>() -> <Extension<T> as HasExtractor>::Extractor
where
    T: Send + Sync + 'static,
{
    Extension::extractor()
}

impl<T> HasExtractor for Extension<T>
where
    T: Send + Sync + 'static,
{
    type Extractor = ExtensionExtractor<T>;

    fn extractor() -> Self::Extractor {
        ExtensionExtractor(PhantomData)
    }
}

pub struct ExtensionExtractor<T>(PhantomData<fn() -> T>);

impl<T> fmt::Debug for ExtensionExtractor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionExtractor").finish()
    }
}

impl<T> Extractor for ExtensionExtractor<T>
where
    T: Send + Sync + 'static,
{
    type Output = (Extension<T>,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        if input.extensions().get::<T>().is_some() {
            Ok(Extract::Ready((Extension::new(),)))
        } else {
            Err(crate::error::internal_server_error("missing extension"))
        }
    }
}

// ==== State ====

pub fn state<T>() -> <State<T> as HasExtractor>::Extractor
where
    T: Send + Sync + 'static,
{
    State::extractor()
}

impl<T> HasExtractor for State<T>
where
    T: Send + Sync + 'static,
{
    type Extractor = StateExtractor<T>;

    fn extractor() -> Self::Extractor {
        StateExtractor(PhantomData)
    }
}

pub struct StateExtractor<T>(PhantomData<fn() -> T>);

impl<T> fmt::Debug for StateExtractor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateExtractor").finish()
    }
}

impl<T> Extractor for StateExtractor<T>
where
    T: Send + Sync + 'static,
{
    type Output = (State<T>,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        if input.state::<T>().is_some() {
            Ok(Extract::Ready((State::new(),)))
        } else {
            Err(crate::error::internal_server_error("missing state"))
        }
    }
}

// ==== Local ====

pub fn local<T>(key: &'static LocalKey<T>) -> LocalExtractor<T>
where
    T: Send + 'static,
{
    LocalExtractor::new(key)
}

#[derive(Debug)]
pub struct LocalExtractor<T>
where
    T: Send + 'static,
{
    key: &'static LocalKey<T>,
}

impl<T> LocalExtractor<T>
where
    T: Send + 'static,
{
    pub fn new(key: &'static LocalKey<T>) -> Self {
        Self { key }
    }
}

impl<T> Extractor for LocalExtractor<T>
where
    T: Send + 'static,
{
    type Output = (T,);
    type Error = Error;
    type Future = super::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        if let Some(value) = input.locals_mut().remove(self.key) {
            Ok(Extract::Ready((value,)))
        } else {
            Err(crate::error::internal_server_error("missing local value"))
        }
    }
}
