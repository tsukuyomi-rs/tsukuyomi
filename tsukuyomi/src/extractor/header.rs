//! Extractors for accessing HTTP header fields.

use {
    super::Extractor,
    crate::{error::Error, future::TryFuture, input::header::HeaderField, util::Never},
    http::header::{HeaderMap, HeaderName, HeaderValue},
};

/// Creates an `Extractor` that parses a header field and returns its result.
pub fn parse<H>() -> impl Extractor<
    Output = (H::Value,), //
    Error = Error,
    Extract = impl TryFuture<Ok = (H::Value,), Error = Error> + Send + 'static,
>
where
    H: HeaderField,
    H::Value: Clone,
{
    super::ready(move |input| {
        crate::input::header::parse::<H>(input)?
            .cloned()
            .map(|h| (h,))
            .ok_or_else(|| crate::error::bad_request(format!("missing header field: {}", H::NAME)))
    })
}

/// Creates an `Extractor` that checks if a header field equals to the specified value.
pub fn equals<T>(
    name: HeaderName,
    value: T,
) -> impl Extractor<
    Output = (), //
    Error = Error,
    Extract = impl TryFuture<Ok = (), Error = Error> + Send + 'static,
>
where
    T: PartialEq<HeaderValue> + Clone + Send + 'static,
{
    super::ready(move |input| match input.request.headers().get(&name) {
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

/// Creates an `Extractor` that clones the entire of header map and returns it.
pub fn headers() -> impl Extractor<
    Output = (HeaderMap,), //
    Error = Never,
    Extract = impl TryFuture<Ok = (HeaderMap,), Error = Never> + Send + 'static,
> {
    super::ready(|input| Ok((input.request.headers().clone(),)))
}
