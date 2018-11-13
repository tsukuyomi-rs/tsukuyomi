//! Extractors for accessing the request-local data.

use std::fmt;

use crate::error::Error;
use crate::extractor::Extractor;
use crate::input::local_map::LocalKey;

pub struct Local<T: Send + 'static> {
    key: &'static LocalKey<T>,
}

impl<T> fmt::Debug for Local<T>
where
    T: Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Local").finish()
    }
}

impl<T> Local<T>
where
    T: Send + 'static,
{
    #[allow(missing_docs)]
    pub fn with<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let x = input
                .locals_mut()
                .get_mut(self.key)
                .expect("should be exist");
            f(x)
        })
    }
}

pub fn by_ref<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (Local<T>,), Error = Error>
where
    T: Send + 'static,
{
    super::ready(move |input| {
        if input.locals_mut().contains_key(key) {
            Ok(Local { key })
        } else {
            Err(crate::error::internal_server_error("missing local value"))
        }
    })
}

pub fn remove<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
where
    T: Send + 'static,
{
    super::ready(move |input| {
        input
            .locals_mut()
            .remove(key)
            .ok_or_else(|| crate::error::internal_server_error("missing local value"))
    })
}

pub fn clone<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
where
    T: Clone + Send + 'static,
{
    super::ready(move |input| {
        input
            .locals_mut()
            .get(key)
            .cloned()
            .ok_or_else(|| crate::error::internal_server_error("missing local value"))
    })
}
