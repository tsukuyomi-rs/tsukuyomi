//! Extractors for accessing HTTP header fields.

use {
    super::Extractor,
    crate::{core::Never, error::Error, input::header::HeaderField},
    http::header::{HeaderMap, HeaderName, HeaderValue},
};

pub fn parse<H>() -> impl Extractor<
    Output = (H::Value,), //
    Error = Error,
    Future = futures01::future::FutureResult<(H::Value,), Error>,
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
) -> impl Extractor<Output = (), Error = Error, Future = futures01::future::FutureResult<(), Error>>
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
    Output = (HeaderMap,),
    Error = Never,
    Future = futures01::future::FutureResult<(HeaderMap,), Never>,
> {
    super::ready(|input| Ok(input.request.headers().clone()))
}
