//! The definition of `Modifier`.
//!
//! The purpose of this trait is to insert some processes before and after
//! applying `Handler` in a certain scope.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use tsukuyomi::{
//!     app::route,
//!     output::Output,
//!     handler::AsyncResult,
//!     Modifier,
//! };
//!
//! #[derive(Default)]
//! struct RequestCounter(AtomicUsize);
//!
//! impl Modifier for RequestCounter {
//!     fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
//!        self.0.fetch_add(1, Ordering::SeqCst);
//!        result
//!     }
//! }
//!
//! # fn main() -> tsukuyomi::app::Result<()> {
//! tsukuyomi::app!()
//!     .with(route!().reply(|| "Hello"))
//!     .with(tsukuyomi::app::scope::modifier(RequestCounter::default()))
//!     .build()
//! #   .map(drop)
//! # }
//! ```

use crate::{handler::AsyncResult, output::Output};

/// A trait representing a `Modifier`.
pub trait Modifier {
    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output>;

    fn chain<M>(self, next: M) -> Chain<Self, M>
    where
        Self: Sized,
        M: Modifier,
    {
        Chain::new(self, next)
    }
}

impl Modifier for () {
    #[inline]
    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
        result
    }
}

#[derive(Debug)]
pub struct Chain<M1, M2> {
    m1: M1,
    m2: M2,
}

impl<M1, M2> Chain<M1, M2> {
    pub fn new(m1: M1, m2: M2) -> Self {
        Self { m1, m2 }
    }
}

impl<M1, M2> Modifier for Chain<M1, M2>
where
    M1: Modifier,
    M2: Modifier,
{
    fn modify(&self, result: AsyncResult<Output>) -> AsyncResult<Output> {
        self.m1.modify(self.m2.modify(result))
    }
}
