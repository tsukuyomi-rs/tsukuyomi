//! Definition of commonly used components.

use std::{error::Error as StdError, fmt};

/// A helper type which emulates the standard `never_type` (`!`).
#[allow(clippy::empty_enum)]
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

/// A trait representing the conversion from the arbitrary types.
///
/// This trait is an emulation of the standard [`TryFrom`].
///
/// [`TryFrom`]: https://doc.rust-lang.org/nightly/std/convert/trait.TryFrom.html
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
