//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod and;
mod fallible;
mod from_input;
mod generic;
mod optional;
mod or;

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

pub use self::and::And;
pub use self::fallible::Fallible;
pub use self::from_input::{
    extension, local, method, state, uri, version, HasExtractor, LocalExtractor,
};
pub(crate) use self::generic::{Combine, Func, Tuple};
pub use self::optional::Optional;
pub use self::or::Or;

// ==== impl ====

use std::fmt;
use std::marker::PhantomData;

use futures::future;
use futures::{Async, Future, Poll};

use crate::error::{Error, Never};
use crate::input::Input;

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct Placeholder<T, E>(PhantomData<fn() -> (T, E)>);

impl<T, E> Future for Placeholder<T, E> {
    type Item = T;
    type Error = E;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unreachable!("The implementation of Extractor is wrong.")
    }
}

pub trait Extractor {
    type Output: Tuple;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error>;
}

impl Extractor for () {
    type Output = ();
    type Error = Never;
    type Future = Placeholder<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, _: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        Ok(Extract::Ready(()))
    }
}

pub enum Extract<E: Extractor + ?Sized> {
    Ready(E::Output),
    Incomplete(E::Future),
}

impl<E> fmt::Debug for Extract<E>
where
    E: Extractor + ?Sized,
    E::Output: fmt::Debug,
    E::Future: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Extract::Ready(ref out) => f.debug_tuple("Ready").field(out).finish(),
            Extract::Incomplete(ref cx) => f.debug_tuple("Incomplete").field(cx).finish(),
        }
    }
}

// ==== ExtractorExt ====

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait ExtractorExt: Extractor + Sized {
    fn optional<T>(self) -> Optional<Self>
    where
        Self: Extractor<Output = (T,)>,
    {
        Optional(self)
    }

    fn fallible<T>(self) -> Fallible<Self>
    where
        Self: Extractor<Output = (T,)>,
    {
        Fallible(self)
    }

    fn and<E>(self, other: E) -> And<Self, E>
    where
        E: Extractor,
        Self::Output: Combine<E::Output>,
    {
        And {
            left: self,
            right: other,
        }
    }

    fn or<E, T, U>(self, other: E) -> Or<Self, E>
    where
        Self: Extractor<Output = (T,)>,
        E: Extractor<Output = (U,)>,
    {
        Or {
            left: self,
            right: other,
        }
    }
}

impl<E: Extractor> ExtractorExt for E {}
