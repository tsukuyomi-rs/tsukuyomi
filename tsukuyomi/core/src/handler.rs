//! `Handler` and supplemental components.

use either::Either;
use futures::{Async, Poll};
use std::fmt;
use std::sync::Arc;

use crate::error::Error;
use crate::input::Input;
use crate::output::Output;

/// A trait representing handler functions.
pub trait Handler {
    /// Applies an incoming request to this handler.
    fn handle(&self, input: &mut Input<'_>) -> Handle;
}

impl<F> Handler for F
where
    F: Fn(&mut Input<'_>) -> Handle,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        (*self)(input)
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        (**self).handle(input)
    }
}

impl<L, R> Handler for Either<L, R>
where
    L: Handler,
    R: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        match self {
            Either::Left(ref handler) => handler.handle(input),
            Either::Right(ref handler) => handler.handle(input),
        }
    }
}

/// A helper function which creates a `Handler` from the specified closure.
pub fn raw(f: impl Fn(&mut Input<'_>) -> Handle) -> impl Handler {
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F> Handler for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> Handle,
    {
        #[inline]
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            (self.0)(input)
        }
    }

    Raw(f)
}

// ----------------------------------------------------------------------------

/// A type representing the return value from `Handler::handle`.
pub struct Handle(HandleKind);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Polling(Box<dyn FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            HandleKind::Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            HandleKind::Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl Handle {
    /// Creates a `Handle` from an immediately value.
    pub fn ready(result: Result<Output, Error>) -> Self {
        Handle(HandleKind::Ready(Some(result)))
    }

    /// Creates a `Handle` from a closure representing an asynchronous computation.
    pub fn polling<F>(f: F) -> Self
    where
        F: FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static,
    {
        Handle(HandleKind::Polling(Box::new(f)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Output, Error> {
        match self.0 {
            HandleKind::Ready(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            HandleKind::Polling(ref mut f) => (f)(input),
        }
    }
}
