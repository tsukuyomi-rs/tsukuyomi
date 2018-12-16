//! Extractors for accessing HTTP header fields.

use {
    super::Extractor,
    crate::{core::Never, error::Error, input::header::HeaderField},
    futures01::Future,
    http::header::{HeaderMap, HeaderName, HeaderValue},
};

pub fn parse<H>() -> impl Extractor<
    Output = (H::Value,), //
    Error = Error,
    Future = impl Future<Item = (H::Value,), Error = Error> + Send + 'static,
>
where
    H: HeaderField,
    H::Value: Clone,
{
    super::ready(move |input| {
        crate::input::header::parse::<H>(input)?
            .cloned()
            .ok_or_else(|| crate::error::bad_request(format!("missing header field: {}", H::NAME)))
    })
}

pub fn exact<T>(
    name: HeaderName,
    value: T,
) -> impl Extractor<
    Output = (), //
    Error = Error,
    Future = impl Future<Item = (), Error = Error> + Send + 'static,
>
where
    T: PartialEq<HeaderValue>,
{
    super::guard(move |input| match input.request.headers().get(&name) {
        Some(h) if value.eq(h) => Ok(()),
        Some(..) => Err(crate::error::bad_request(format!(
            "mismatched header field: {}",
            name
        ))),
        None => Err(crate::error::bad_request(format!(
            "missing header field: {}",
            name
        ))),
    })
}

pub fn clone_headers() -> impl Extractor<
    Output = (HeaderMap,), //
    Error = Never,
    Future = impl Future<Item = (HeaderMap,), Error = Never> + Send + 'static,
> {
    super::ready(|input| Ok(input.request.headers().clone()))
}
