use {
    crate::{error::HttpError, input::Input, output::Output},
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

/// A pair of structs representing arbitrary chain structure.
#[derive(Debug, Clone)]
pub struct Chain<L, R> {
    pub(crate) left: L,
    pub(crate) right: R,
}

impl<L, R> Chain<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}
