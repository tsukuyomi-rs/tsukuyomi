//! Components for accessing HTTP requests and global/request-local data.

pub mod body;
pub mod localmap;

pub use {
    self::body::RequestBody,
    crate::app::imp::{Cookies, Input, Params},
};

use {
    std::{borrow::Cow, str::Utf8Error},
    url::percent_encoding::percent_decode,
};

#[derive(Debug)]
#[repr(C)]
pub struct PercentEncoded(str);

impl PercentEncoded {
    pub unsafe fn new_unchecked(s: &str) -> &Self {
        &*(s as *const str as *const Self)
    }

    pub fn decode_utf8(&self) -> Result<Cow<'_, str>, Utf8Error> {
        percent_decode(self.0.as_bytes()).decode_utf8()
    }

    pub fn decode_utf8_lossy(&self) -> Cow<'_, str> {
        percent_decode(self.0.as_bytes()).decode_utf8_lossy()
    }
}

pub trait FromPercentEncoded: Sized {
    type Error: Into<crate::Error>;

    fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error>;
}

macro_rules! impl_from_percent_encoded {
    ($($t:ty),*) => {$(
        impl FromPercentEncoded for $t {
            type Error = crate::Error;

            #[inline]
            fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error> {
                s.decode_utf8()
                    .map_err(crate::error::bad_request)?
                    .parse()
                    .map_err(crate::error::bad_request)
            }
        }
    )*};
}

impl_from_percent_encoded!(bool, char, f32, f64, String);
impl_from_percent_encoded!(i8, i16, i32, i64, i128, isize);
impl_from_percent_encoded!(u8, u16, u32, u64, u128, usize);
impl_from_percent_encoded!(
    std::net::SocketAddr,
    std::net::SocketAddrV4,
    std::net::SocketAddrV6,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr,
    url::Url,
    uuid::Uuid
);

impl FromPercentEncoded for std::path::PathBuf {
    type Error = crate::Error;

    #[inline]
    fn from_percent_encoded(s: &PercentEncoded) -> Result<Self, Self::Error> {
        s.decode_utf8()
            .map(|s| Self::from(s.into_owned()))
            .map_err(crate::error::bad_request)
    }
}
