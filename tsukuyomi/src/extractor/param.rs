//! Extractors for parsing parameters in HTTP path.

#![allow(missing_docs)]

use {
    super::Extractor,
    crate::error::Error,
    std::{borrow::Cow, path::PathBuf, str::Utf8Error},
    url::percent_encoding::percent_decode,
    uuid::Uuid,
};

#[derive(Debug)]
pub struct EncodedStr<'a>(&'a str);

impl<'a> EncodedStr<'a> {
    pub fn decode_utf8(&self) -> Result<Cow<'a, str>, Utf8Error> {
        percent_decode(self.0.as_bytes()).decode_utf8()
    }

    pub fn decode_utf8_lossy(&self) -> Cow<'a, str> {
        percent_decode(self.0.as_bytes()).decode_utf8_lossy()
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait FromParam: Sized + Send + 'static {
    type Error: Into<Error>;

    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error>;
}

macro_rules! impl_from_param {
    ($($t:ty),*) => {$(
        impl FromParam for $t {
            type Error = Error;

            #[inline]
            fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
                s.decode_utf8()
                    .map_err(crate::error::bad_request)?
                    .parse()
                    .map_err(crate::error::bad_request)
            }
        }
    )*};
}

impl_from_param!(bool, char, f32, f64, String);
impl_from_param!(i8, i16, i32, i64, i128, isize);
impl_from_param!(u8, u16, u32, u64, u128, usize);
impl_from_param!(
    std::net::SocketAddr,
    std::net::SocketAddrV4,
    std::net::SocketAddrV6,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr,
    url::Url,
    Uuid
);

impl FromParam for PathBuf {
    type Error = Error;

    #[inline]
    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
        s.decode_utf8()
            .map(|s| Self::from(s.into_owned()))
            .map_err(crate::error::bad_request)
    }
}

pub fn pos<T>(pos: usize) -> impl Extractor<Output = (T,), Error = Error>
where
    T: FromParam,
{
    super::ready(move |input| {
        let params = input.params();
        let s = params
            .get(pos)
            .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
        T::from_param(EncodedStr(s)).map_err(Into::into)
    })
}

pub fn named<T>(name: impl Into<String>) -> impl Extractor<Output = (T,), Error = Error>
where
    T: FromParam,
{
    let name = name.into();
    super::ready(move |input| {
        let params = input.params();
        let s = params
            .name(&name)
            .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
        T::from_param(EncodedStr(s)).map_err(Into::into)
    })
}

pub fn wildcard<T>() -> impl Extractor<Output = (T,), Error = Error>
where
    T: FromParam,
{
    super::ready(move |input| {
        let params = input.params();
        let s = params.get_wildcard().ok_or_else(|| {
            crate::error::internal_server_error("the wildcard parameter is not set")
        })?;
        T::from_param(EncodedStr(s)).map_err(Into::into)
    })
}
