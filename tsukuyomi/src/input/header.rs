use {
    super::{
        localmap::{local_key, Entry, LocalKey},
        Input,
    },
    crate::error::Error,
    http::header::{HeaderName, HeaderValue},
    mime::Mime,
};

pub trait FromHeaderValue: Sized {
    type Error: Into<Error>;

    fn from_header_value(h: &HeaderValue) -> Result<Self, Self::Error>;
}

impl FromHeaderValue for String {
    type Error = Error;

    #[inline]
    fn from_header_value(h: &HeaderValue) -> Result<Self, Self::Error> {
        Self::from_utf8(h.as_bytes().to_vec()).map_err(crate::error::bad_request)
    }
}

impl FromHeaderValue for Mime {
    type Error = Error;

    #[inline]
    fn from_header_value(h: &HeaderValue) -> Result<Self, Self::Error> {
        h.to_str()
            .map_err(crate::error::bad_request)?
            .parse()
            .map_err(crate::error::bad_request)
    }
}

pub trait HeaderField {
    type Value: FromHeaderValue + Send + 'static;
    const NAME: &'static HeaderName;
    const KEY: LocalKey<Option<Self::Value>>;
}

#[derive(Debug)]
pub struct ContentType(());

impl HeaderField for ContentType {
    type Value = Mime;

    const NAME: &'static HeaderName = &http::header::CONTENT_TYPE;

    local_key! {
        const KEY: Option<Self::Value>;
    }
}

/// Parses the header field.
pub fn parse<'a, H>(input: &'a mut Input<'_>) -> Result<Option<&'a H::Value>, Error>
where
    H: HeaderField,
{
    match input.locals.entry(&H::KEY) {
        Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
        Entry::Vacant(entry) => {
            let value = match input.request.headers().get(H::NAME) {
                Some(h) => FromHeaderValue::from_header_value(h)
                    .map(Some)
                    .map_err(Into::into)?,
                None => None,
            };
            Ok(entry.insert(value).as_ref())
        }
    }
}
