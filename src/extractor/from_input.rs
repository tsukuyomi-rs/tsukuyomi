use std::cell::UnsafeCell;
use std::fmt;
use std::marker::PhantomData;

use crate::error::{Error, Never};
use crate::extractor::{Extractor, Preflight};
use crate::input::local_map::LocalData;
use crate::input::Input;

/// A trait representing the general data extraction from the incoming request.
pub trait FromInput: Sized + 'static {
    /// The error type which will be returned from `from_input`.
    type Error: Into<Error>;

    /// Extract the data from the current context.
    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error>;
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct Directly<T>(PhantomData<fn() -> T>);

impl<T> Default for Directly<T> {
    fn default() -> Self {
        Directly(PhantomData)
    }
}

impl<T> fmt::Debug for Directly<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Directly").finish()
    }
}

impl<T> Extractor for Directly<T>
where
    T: FromInput,
{
    type Out = T;
    type Error = T::Error;
    type Ctx = ();

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        T::from_input(input).map(Preflight::Completed)
    }
}

// ---- implementors ----

impl FromInput for http::Method {
    type Error = Never;

    #[inline]
    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        Ok(input.method().clone())
    }
}

impl FromInput for http::Uri {
    type Error = Never;

    #[inline]
    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        Ok(input.uri().clone())
    }
}

impl FromInput for http::Version {
    type Error = Never;

    #[inline]
    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        Ok(input.version())
    }
}

/// A proxy object for accessing the value in the protocol extensions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct Extension<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
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

impl<T> FromInput for Extension<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;

    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        if input.extensions().get::<T>().is_some() {
            Ok(Self {
                _marker: PhantomData,
            })
        } else {
            Err(crate::error::internal_server_error("missing extension").into())
        }
    }
}

/// A proxy object for accessing the global state.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct State<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
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

impl<T> FromInput for State<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;

    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        if input.state::<T>().is_some() {
            Ok(Self {
                _marker: PhantomData,
            })
        } else {
            Err(crate::error::internal_server_error("missing state").into())
        }
    }
}

// ==== Local ====

/// A proxy object for accessing local data from handler functions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct Local<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

#[allow(missing_docs)]
impl<T> Local<T>
where
    T: LocalData,
{
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let value = T::get(input.locals()).expect("should be Some");
            f(value)
        })
    }

    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let value = T::get_mut(input.locals_mut()).expect("should be Some");
            f(value)
        })
    }
}

impl<T> FromInput for Local<T>
where
    T: LocalData,
{
    type Error = Error;

    fn from_input(input: &mut Input<'_>) -> Result<Self, Self::Error> {
        if input.locals().contains_key(&T::KEY) {
            Ok(Self {
                _marker: PhantomData,
            })
        } else {
            Err(crate::error::internal_server_error("missing local value").into())
        }
    }
}
