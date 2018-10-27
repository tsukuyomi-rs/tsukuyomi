use std::cell::UnsafeCell;
use std::fmt;
use std::marker::PhantomData;

use crate::error::{Error, Never};
use crate::extractor::{Extractor, Preflight};
use crate::input::local_map::LocalKey;
use crate::input::Input;

pub trait HasExtractor: Sized {
    type Extractor: Extractor<Out = Self>;
    fn extractor() -> Self::Extractor;
}

// ---- implementors ----

pub struct RequestExtractor<T>(fn(&mut Input<'_>) -> T);

impl<T> fmt::Debug for RequestExtractor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestExtractor").finish()
    }
}

impl<T> Extractor for RequestExtractor<T> {
    type Out = T;
    type Error = Never;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        Ok(Preflight::Completed((self.0)(input)))
    }
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

/// A proxy object for accessing the value in the protocol extensions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
pub struct Extension<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> fmt::Debug for Extension<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extension").finish()
    }
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

impl<T> Extension<T>
where
    T: Send + Sync + 'static,
{
    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.extensions().get::<T>().expect("should be exist");
            f(state)
        })
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
    type Out = Extension<T>;
    type Error = Error;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if input.extensions().get::<T>().is_some() {
            Ok(Preflight::Completed(Extension {
                _marker: PhantomData,
            }))
        } else {
            Err(crate::error::internal_server_error("missing extension").into())
        }
    }
}

// ==== State ====

/// A proxy object for accessing the global state.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
pub struct State<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
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

impl<T> State<T>
where
    T: Send + Sync + 'static,
{
    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.state::<T>().expect("should be exist");
            f(state)
        })
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
    type Out = State<T>;
    type Error = Error;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if input.state::<T>().is_some() {
            Ok(Preflight::Completed(State {
                _marker: PhantomData,
            }))
        } else {
            Err(crate::error::internal_server_error("missing state").into())
        }
    }
}

// ==== Local ====

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
    type Out = T;
    type Error = Error;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if let Some(value) = input.locals_mut().remove(self.key) {
            Ok(Preflight::Completed(value))
        } else {
            Err(crate::error::internal_server_error("missing local value").into())
        }
    }
}
