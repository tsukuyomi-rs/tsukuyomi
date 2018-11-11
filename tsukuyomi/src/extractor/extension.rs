use std::cell::UnsafeCell;
use std::fmt;
use std::marker::PhantomData;

use crate::error::Error;
use crate::extractor::Extractor;

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

impl<T> Extension<T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    #[allow(missing_docs)]
    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        crate::input::with_get_current(|input| {
            let state = input.extensions().get::<T>().expect("should be exist");
            f(state)
        })
    }
}

pub fn extension<T>() -> impl Extractor<Output = (Extension<T>,), Error = Error>
where
    T: Send + Sync + 'static,
{
    super::ready(|input| {
        if input.extensions().get::<T>().is_some() {
            Ok(Extension::new())
        } else {
            Err(crate::error::internal_server_error("missing extension"))
        }
    })
}

pub fn cloned<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        if let Some(ext) = input.extensions().get::<T>() {
            Ok(ext.clone())
        } else {
            Err(crate::error::internal_server_error("missing extension"))
        }
    })
}
