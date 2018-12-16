use {
    crate::{app::Captures, uri::CaptureNames},
    std::borrow::Cow,
    std::ops::Index,
    std::str::Utf8Error,
    url::percent_encoding::percent_decode,
};

/// A proxy object for accessing extracted parameters.
#[derive(Debug)]
pub struct Params<'input> {
    pub(crate) path: &'input str,
    pub(crate) names: Option<&'input CaptureNames>,
    pub(crate) captures: Option<&'input Captures>,
}

impl<'input> Params<'input> {
    /// Returns `true` if the extracted paramater exists.
    pub fn is_empty(&self) -> bool {
        self.captures.map_or(true, |captures| {
            captures.params().is_empty() && captures.wildcard().is_none()
        })
    }

    /// Returns the value of `i`-th parameter, if exists.
    pub fn get(&self, i: usize) -> Option<&str> {
        let &(s, e) = self.captures?.params().get(i)?;
        self.path.get(s..e)
    }

    /// Returns the value of catch-all parameter, if exists.
    pub fn catch_all(&self) -> Option<&str> {
        let (s, e) = self.captures?.wildcard()?;
        self.path.get(s..e)
    }

    /// Returns the value of parameter whose name is equal to `name`, if exists.
    pub fn name(&self, name: &str) -> Option<&str> {
        match name {
            "*" => self.catch_all(),
            name => self.get(self.names?.position(name)?),
        }
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}

impl<'input, 'a> Index<&'a str> for Params<'input> {
    type Output = str;

    fn index(&self, name: &'a str) -> &Self::Output {
        self.name(name).expect("Out of range")
    }
}

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
