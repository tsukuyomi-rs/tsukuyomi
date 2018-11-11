use crate::error::Error;
use crate::extractor::Extractor;
use crate::input::local_map::LocalKey;

pub fn local<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
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

pub fn optional<T>(
    key: &'static LocalKey<T>,
) -> impl Extractor<Output = (Option<T>,), Error = Error>
where
    T: Send + 'static,
{
    super::ready(move |input| Ok(input.locals_mut().remove(key)))
}

pub fn cloned<T>(key: &'static LocalKey<T>) -> impl Extractor<Output = (T,), Error = Error>
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

pub fn cloned_optional<T>(
    key: &'static LocalKey<T>,
) -> impl Extractor<Output = (Option<T>,), Error = Error>
where
    T: Clone + Send + 'static,
{
    super::ready(move |input| Ok(input.locals_mut().get(key).cloned()))
}
