//! Extractors for parsing parameters in HTTP path.

#![allow(missing_docs)]

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::{FromStr, Utf8Error};
use url::percent_encoding::percent_decode;
use uuid::Uuid;

use crate::error::Error;
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

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
    type Error: fmt::Debug + fmt::Display + Send + 'static;

    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error>;
}

#[doc(hidden)]
#[derive(Debug)]
pub enum FromParamError<E> {
    Decode(Utf8Error),
    Parse(E),
}

impl<E> fmt::Display for FromParamError<E>
where
    E: fmt::Debug + fmt::Display + Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FromParamError::Decode(e) => fmt::Display::fmt(e, f),
            FromParamError::Parse(e) => fmt::Display::fmt(e, f),
        }
    }
}

macro_rules! impl_from_param {
    ($($t:ty),*) => {$(
        impl FromParam for $t {
            type Error = FromParamError<<$t as FromStr>::Err>;

            #[inline]
            fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
                s.decode_utf8()
                    .map_err(FromParamError::Decode)?
                    .parse()
                    .map_err(FromParamError::Parse)
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
    type Error = Utf8Error;

    #[inline]
    fn from_param(s: EncodedStr<'_>) -> Result<Self, Self::Error> {
        s.decode_utf8().map(|s| Self::from(s.into_owned()))
    }
}

pub fn pos<T>(pos: usize) -> impl Extractor<Output = (T,)>
where
    T: FromParam,
{
    #[derive(Debug)]
    struct Pos<T> {
        pos: usize,
        _marker: PhantomData<fn() -> T>,
    }

    impl<T> Extractor for Pos<T>
    where
        T: FromParam,
    {
        type Output = (T,);
        type Error = Error;
        type Future = super::Placeholder<Self::Output, Self::Error>;

        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            let params = input.params();
            let s = params
                .get(self.pos)
                .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
            T::from_param(EncodedStr(s))
                .map(|out| Extract::Ready((out,)))
                .map_err(crate::error::bad_request)
        }
    }

    Pos {
        pos,
        _marker: PhantomData,
    }
}

pub fn named<T>(name: impl Into<String>) -> impl Extractor<Output = (T,)>
where
    T: FromParam,
{
    #[derive(Debug)]
    struct Named<T> {
        name: String,
        _marker: PhantomData<fn() -> T>,
    }

    impl<T> Extractor for Named<T>
    where
        T: FromParam,
    {
        type Output = (T,);
        type Error = Error;
        type Future = super::Placeholder<Self::Output, Self::Error>;

        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            let params = input.params();
            let s = params
                .name(&self.name)
                .ok_or_else(|| crate::error::internal_server_error("the cursor is out of range"))?;
            T::from_param(EncodedStr(s))
                .map(|out| Extract::Ready((out,)))
                .map_err(crate::error::bad_request)
        }
    }

    Named {
        name: name.into(),
        _marker: PhantomData,
    }
}

pub fn wildcard<T>() -> impl Extractor<Output = (T,)>
where
    T: FromParam,
{
    #[derive(Debug)]
    struct Wildcard<T> {
        _marker: PhantomData<fn() -> T>,
    }

    impl<T> Extractor for Wildcard<T>
    where
        T: FromParam,
    {
        type Output = (T,);
        type Error = Error;
        type Future = super::Placeholder<Self::Output, Self::Error>;

        fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
            let params = input.params();
            let s = params.get_wildcard().ok_or_else(|| {
                crate::error::internal_server_error("the wildcard parameter is not set")
            })?;
            T::from_param(EncodedStr(s))
                .map(|out| Extract::Ready((out,)))
                .map_err(crate::error::bad_request)
        }
    }

    Wildcard {
        _marker: PhantomData,
    }
}
