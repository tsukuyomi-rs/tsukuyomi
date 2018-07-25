//! `Modifier` and supplemental components.
//!
//! The purpose of `Modifier` is to insert some processes before and after
//! applying `Handler` in a certain scope.
//!
//! # Examples
//!
//! ```
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use tsukuyomi::{App, Input, handler};
//! use tsukuyomi::modifier::{Modifier, BeforeHandle, AfterHandle};
//!
//! #[derive(Default)]
//! struct RequestCounter(AtomicUsize);
//!
//! impl Modifier for RequestCounter {
//!     fn before_handle(&self, _: &mut Input) -> BeforeHandle {
//!        self.0.fetch_add(1, Ordering::SeqCst);
//!        BeforeHandle::ready(Ok(None))
//!     }
//! }
//!
//! # fn main() -> tsukuyomi::AppResult<()> {
//! let app = App::builder()
//!     .route(("/", handler::wrap_ready(|_| "Hello")))
//!     .modifier(RequestCounter::default())
//!     .finish()?;
//! # Ok(())
//! # }
//! ```

use futures::{self, Future, Poll};
use std::fmt;

use error::Error;
use input::{self, Input};
use output::Output;

/// A trait representing a `Modifier`.
///
/// See the module level documentation for details.
pub trait Modifier {
    /// Performs the process before calling the handler.
    ///
    /// By default, this method does nothing.
    #[allow(unused_variables)]
    fn before_handle(&self, input: &mut Input) -> BeforeHandle {
        BeforeHandle::ready(Ok(None))
    }

    /// Modifies the returned value from a handler.
    ///
    /// By default, this method does nothing and immediately return the provided `Output`.
    #[allow(unused_variables)]
    fn after_handle(&self, input: &mut Input, result: Result<Output, Error>) -> AfterHandle {
        AfterHandle::ready(result)
    }
}

// ==== BeforeHandle ====

/// The type representing a return value from `Modifier::before_handle`.
#[derive(Debug)]
pub struct BeforeHandle(BeforeHandleState);

// MEMO:
// The internal type should be replaced with `Option<Result<Output, Error>>`.
// Currently, it is represented as `Result<T, E>` due to the restriction of `futures`.
#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum BeforeHandleState {
    Ready(Option<Result<Option<Output>, Error>>),
    Polling(Box<dyn FnMut(&mut Input) -> Poll<Option<Output>, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for BeforeHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BeforeHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl<E> From<Result<Option<Output>, E>> for BeforeHandle
where
    Error: From<E>,
{
    fn from(result: Result<Option<Output>, E>) -> BeforeHandle {
        BeforeHandle::ready(result.map_err(Into::into))
    }
}

impl BeforeHandle {
    /// Creates a `BeforeHandle` from an immediately value.
    pub fn ready(result: Result<Option<Output>, Error>) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Ready(Some(result)))
    }

    /// Creates a `BeforeHandle` from a closure repsenting an asynchronous computation.
    pub fn polling(f: impl FnMut(&mut Input) -> Poll<Option<Output>, Error> + Send + 'static) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Polling(Box::new(f)))
    }

    /// Creates a `BeforeHandle` from a future.
    #[inline(always)]
    pub fn wrap_future(mut future: impl Future<Item = Option<Output>, Error = Error> + Send + 'static) -> BeforeHandle {
        BeforeHandle::polling(move |input| input::with_set_current(input, || future.poll()))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Option<Output>, Error> {
        use self::BeforeHandleState::*;
        match self.0 {
            Ready(ref mut res) => res.take()
                .expect("BeforeHandle has already polled")
                .map(futures::Async::Ready),
            Polling(ref mut f) => f(input),
        }
    }
}

// ==== AfterHandle ====

/// The type representing a return value from `Modifier::after_handle`.
#[derive(Debug)]
pub struct AfterHandle(AfterHandleState);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum AfterHandleState {
    Ready(Option<Result<Output, Error>>),
    Polling(Box<dyn FnMut(&mut Input) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AfterHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AfterHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl<T, E> From<Result<T, E>> for AfterHandle
where
    T: Into<Output>,
    Error: From<E>,
{
    fn from(result: Result<T, E>) -> AfterHandle {
        AfterHandle::ready(result.map(Into::into).map_err(Into::into))
    }
}

impl AfterHandle {
    /// Creates an `AfterHandle` from an immediately value.
    pub fn ready(result: Result<Output, Error>) -> AfterHandle {
        AfterHandle(AfterHandleState::Ready(Some(result)))
    }

    /// Creates an `AfterHandle` from a closure repsenting an asynchronous computation.
    pub fn polling(f: impl FnMut(&mut Input) -> Poll<Output, Error> + Send + 'static) -> AfterHandle {
        AfterHandle(AfterHandleState::Polling(Box::new(f)))
    }

    /// Creates an `AfterHandle` from a `Future`.
    #[inline(always)]
    pub fn wrap_future(mut future: impl Future<Item = Output, Error = Error> + Send + 'static) -> AfterHandle {
        AfterHandle::polling(move |input| input::with_set_current(input, || future.poll()))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Output, Error> {
        use self::AfterHandleState::*;
        match self.0 {
            Ready(ref mut res) => res.take()
                .expect("AfterHandle has already polled")
                .map(futures::Async::Ready),
            Polling(ref mut f) => (*f)(input),
        }
    }
}
