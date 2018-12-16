//! Miscellaneous components used within the framework.

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

/// A trait representing the conversion into a value of specified type.
///
/// This trait is an emulation of the standard [`TryInto`].
///
/// [`TryInto`]: https://doc.rust-lang.org/nightly/std/convert/trait.TryInto.html
pub trait TryInto<T> {
    type Error: Into<failure::Error>;

    fn try_into(self) -> Result<T, Self::Error>;
}

impl<T, U> TryInto<U> for T
where
    U: TryFrom<T>,
{
    type Error = <U as TryFrom<T>>::Error;

    #[inline]
    fn try_into(self) -> Result<U, Self::Error> {
        U::try_from(self)
    }
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

/// A macro for creating a chain of expressions.
#[macro_export]
macro_rules! chain {
    ($e:expr) => ( $e );
    ($e:expr, ) => ( $e );
    ($e1:expr, $e2:expr) => ( $crate::util::Chain::new($e1, $e2) );
    ($e1:expr, $e2:expr, $($t:expr),*) => {
        $crate::util::Chain::new($e1, chain!($e2, $($t),*))
    };
    ($e1:expr, $e2:expr, $($t:expr,)+) => ( chain!{ $e1, $e2, $($t),+ } );
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}
