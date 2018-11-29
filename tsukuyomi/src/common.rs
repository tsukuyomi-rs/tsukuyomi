use {
    crate::{error::HttpError, input::Input, output::Output, uri::Uri},
    http::StatusCode,
    std::{error::Error as StdError, fmt},
};

/// A helper type which emulates the standard `never_type` (`!`).
#[cfg_attr(feature = "cargo-clippy", allow(empty_enum))]
#[derive(Debug)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl StdError for Never {
    fn description(&self) -> &str {
        match *self {}
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match *self {}
    }
}

impl HttpError for Never {
    fn status_code(&self) -> StatusCode {
        match *self {}
    }

    fn to_response(&mut self, _: &mut Input<'_>) -> Output {
        match *self {}
    }
}

/// A helper trait representing the conversion into an `Uri`.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait TryFrom<T>: Sized {
    type Error: Into<failure::Error>;

    fn try_from(value: T) -> Result<Self, Self::Error>;
}

impl TryFrom<Self> for Uri {
    type Error = Never;

    #[inline]
    fn try_from(uri: Self) -> Result<Self, Self::Error> {
        Ok(uri)
    }
}

impl<'a> TryFrom<&'a str> for Uri {
    type Error = failure::Error;

    #[inline]
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for Uri {
    type Error = failure::Error;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}
