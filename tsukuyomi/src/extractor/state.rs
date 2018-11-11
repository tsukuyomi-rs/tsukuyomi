use std::cell::UnsafeCell;
use std::fmt;
use std::marker::PhantomData;

use crate::error::Error;
use crate::extractor::Extractor;

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

impl<T> State<T>
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
            let state = input.state::<T>().expect("should be exist");
            f(state)
        })
    }
}

pub fn state<T>() -> impl Extractor<Output = (State<T>,), Error = Error>
where
    T: Send + Sync + 'static,
{
    super::ready(|input| {
        if input.state::<T>().is_some() {
            Ok(State::new())
        } else {
            Err(crate::error::internal_server_error("missing state"))
        }
    })
}

pub fn cloned<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + Sync + 'static,
{
    super::ready(|input| {
        if let Some(state) = input.state::<T>() {
            Ok(state.clone())
        } else {
            Err(crate::error::internal_server_error("missing state"))
        }
    })
}
