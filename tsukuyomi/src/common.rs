use std::{error::Error as StdError, fmt};

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

/// A trait representing the arbitrary conversion into `Self`.
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
