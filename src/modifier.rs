//! Components for supporting modifiers.
//!
//! The main trait for supporting the middlewares is `Modifier`.
//! This trait is used to insert some processes before and/or after calling the handler recognized by the
//! router.
//!
//! NOTE:
//! The purpose of abstraction by using `Modifier` is to provide a *basic* extension for HTTP
//! usage.
//! If you want to do more complex management (such as connection-level logging, load balancing),
//! consider wrapping the instance of `App` and implements `Service` for adding the features from
//! the outside.
//!
//! # Examples
//!
//! ```
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use tsukuyomi::{App, Input, Handler};
//! use tsukuyomi::modifier::{Modifier, BeforeHandle, AfterHandle};
//!
//! #[derive(Default)]
//! struct RequestCounter(AtomicUsize);
//!
//! impl Modifier for RequestCounter {
//!     fn before_handle(&self, _: &mut Input) -> BeforeHandle {
//!        self.0.fetch_add(1, Ordering::SeqCst);
//!        BeforeHandle::ok()
//!     }
//! }
//!
//! let app = App::builder()
//!     .mount("/", |m| {
//!         m.get("/").handle(Handler::new_ready(|_| "Hello"));
//!     })
//!     .modifier(RequestCounter::default())    // <--
//!     .finish()
//!     .unwrap();
//! ```

use std::fmt;

use error::Error;
use future::{Future, Poll};
use input::Input;
use output::Output;

/// A trait representing a `Modifier`.
///
/// A modifier inserts the process before and after calling a handler associated with an endpoint
/// matched to the incoming request, and performs some preprecess independent on the certain route
/// or modifies its response before sending to the peer.
pub trait Modifier {
    /// Performs the process before calling the handler.
    ///
    /// By default, this method does nothing.
    #[allow(unused_variables)]
    fn before_handle(&self, input: &mut Input) -> BeforeHandle {
        BeforeHandle::ok()
    }

    /// Modifies the returned value from a handler.
    ///
    /// By default, this method does nothing and immediately return the provided `Output`.
    #[allow(unused_variables)]
    fn after_handle(&self, input: &mut Input, output: Output) -> AfterHandle {
        AfterHandle::ok(output)
    }
}

// ==== BeforeHandle ====

/// The type representing a return value from `Modifier::before_handle`.
///
/// Roughly speaking, this type is an alias of `Future<Item = Option<Output>, Error = Error>`.
#[derive(Debug)]
pub struct BeforeHandle(BeforeHandleState);

enum BeforeHandleState {
    Ready(Option<Result<Option<Output>, Error>>),
    Async(Box<dyn Future<Output = Result<Option<Output>, Error>> + Send>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for BeforeHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::BeforeHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

impl BeforeHandle {
    fn ready(res: Result<Option<Output>, Error>) -> BeforeHandle {
        BeforeHandle(BeforeHandleState::Ready(Some(res)))
    }

    /// Creates an empty value of `BeforeHandle`.
    ///
    /// When this value is received, the framework continues the subsequent processes.
    pub fn ok() -> BeforeHandle {
        BeforeHandle::ready(Ok(None))
    }

    /// Creates a `BeforeHandle` with the value of an `Output`.
    ///
    /// When this value is received, the framework cancels all processes of remaining modifiers
    /// and the handler of endpoint, and then shifts to the calling `after_handle()` of the
    /// (already applied) modifiers.
    pub fn done<T>(output: T) -> BeforeHandle
    where
        T: Into<Output>,
    {
        BeforeHandle::ready(Ok(Some(output.into())))
    }

    /// Creates a `BeforeHandle` with an error value.
    ///
    /// When this value is received, the framework suspends all remaining processes and immediately
    /// performs the error handling.
    pub fn err<E>(err: E) -> BeforeHandle
    where
        E: Into<Error>,
    {
        BeforeHandle::ready(Err(err.into()))
    }

    /// Creates a `BeforeHandle` from a future.
    pub fn async<F>(future: F) -> BeforeHandle
    where
        F: Future<Output = Result<Option<Output>, Error>> + Send + 'static,
    {
        BeforeHandle(BeforeHandleState::Async(Box::new(future)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Result<Option<Output>, Error>> {
        use self::BeforeHandleState::*;
        match self.0 {
            Ready(ref mut res) => Poll::Ready(res.take().expect("BeforeHandle has already polled")),
            Async(ref mut f) => input.with_set(|| f.poll()),
        }
    }
}

// ==== AfterHandle ====

/// The type representing a return value from `Modifier::after_handle`.
#[derive(Debug)]
pub struct AfterHandle(AfterHandleState);

enum AfterHandleState {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn Future<Output = Result<Output, Error>> + Send>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AfterHandleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AfterHandleState::*;
        match *self {
            Ready(ref res) => f.debug_tuple("Immediate").field(res).finish(),
            Async(..) => f.debug_tuple("Boxed").finish(),
        }
    }
}

impl AfterHandle {
    fn ready(res: Result<Output, Error>) -> AfterHandle {
        AfterHandle(AfterHandleState::Ready(Some(res)))
    }

    /// Creates an `AfterHandle` from an `Output`.
    pub fn ok(output: Output) -> AfterHandle {
        AfterHandle::ready(Ok(output))
    }

    /// Creates an `AfterHandle` from an error value.
    pub fn err<E>(err: E) -> AfterHandle
    where
        E: Into<Error>,
    {
        AfterHandle::ready(Err(err.into()))
    }

    /// Creates an `AfterHandle` from a future.
    pub fn async<F>(future: F) -> AfterHandle
    where
        F: Future<Output = Result<Output, Error>> + Send + 'static,
    {
        AfterHandle(AfterHandleState::Async(Box::new(future)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Result<Output, Error>> {
        use self::AfterHandleState::*;
        match self.0 {
            Ready(ref mut res) => Poll::Ready(res.take().expect("AfterHandle has already polled")),
            Async(ref mut f) => input.with_set(|| f.poll()),
        }
    }
}
