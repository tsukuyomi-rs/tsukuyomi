//! Definition of `Handler` and `Modifier`.

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
    fn handle(&self, input: &mut Input<'_>) -> AsyncResult<Output>;
}

impl<F, R> Handler for F
where
    F: Fn(&mut Input<'_>) -> R,
    R: Into<AsyncResult<Output>>,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> AsyncResult<Output> {
        (*self)(input).into()
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> AsyncResult<Output> {
        (**self).handle(input)
    }
}

impl<L, R> Handler for Either<L, R>
where
    L: Handler,
    R: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> AsyncResult<Output> {
        match self {
            Either::Left(ref handler) => handler.handle(input),
            Either::Right(ref handler) => handler.handle(input),
        }
    }
}

/// A helper function which creates a `Handler` from the specified closure.
pub fn raw<F, R>(f: F) -> impl Handler
where
    F: Fn(&mut Input<'_>) -> R,
    R: Into<AsyncResult<Output>>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Handler for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> R,
        R: Into<AsyncResult<Output>>,
    {
        #[inline]
        fn handle(&self, input: &mut Input<'_>) -> AsyncResult<Output> {
            (self.0)(input).into()
        }
    }

    Raw(f)
}

/// A trait representing a `Modifier`.
///
/// The purpose of this trait is to insert some processes before and after
/// applying `Handler` in a certain scope.
///
/// # Examples
///
/// ```
/// # extern crate tsukuyomi;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use tsukuyomi::app::{route, Modifier, AsyncResult};
/// use tsukuyomi::input::Input;
/// use tsukuyomi::output::Output;
///
/// #[derive(Default)]
/// struct RequestCounter(AtomicUsize);
///
/// impl Modifier for RequestCounter {
///     fn before_handle(&self, _: &mut Input) -> AsyncResult<Option<Output>> {
///        self.0.fetch_add(1, Ordering::SeqCst);
///        AsyncResult::ready(Ok(None))
///     }
/// }
///
/// # fn main() -> tsukuyomi::app::Result<()> {
/// tsukuyomi::app()
///     .route(route!().reply(|| "Hello"))
///     .modifier(RequestCounter::default())
///     .build()
/// #   .map(drop)
/// # }
/// ```
pub trait Modifier {
    /// Performs the process before calling the handler.
    ///
    /// By default, this method does nothing.
    #[allow(unused_variables)]
    #[cfg_attr(tarpaulin, skip)]
    fn before_handle(&self, input: &mut Input<'_>) -> AsyncResult<Option<Output>> {
        AsyncResult::ready(Ok(None))
    }

    /// Modifies the returned value from a handler.
    ///
    /// By default, this method does nothing and immediately return the provided `Output`.
    #[allow(unused_variables)]
    #[cfg_attr(tarpaulin, skip)]
    fn after_handle(
        &self,
        input: &mut Input<'_>,
        result: Result<Output, Error>,
    ) -> AsyncResult<Output> {
        AsyncResult::ready(result)
    }
}

/// A type representing the return value from `Handler::handle`.
pub struct AsyncResult<T, E = Error>(AsyncResultKind<T, E>);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum AsyncResultKind<T, E> {
    Ready(Option<Result<T, E>>),
    Polling(Box<dyn FnMut(&mut Input<'_>) -> Poll<T, E> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl<T, E> fmt::Debug for AsyncResult<T, E>
where
    T: fmt::Debug,
    E: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            AsyncResultKind::Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            AsyncResultKind::Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl<T, E> From<Result<T, E>> for AsyncResult<T, E>
where
    Error: From<E>,
{
    fn from(result: Result<T, E>) -> Self {
        Self::ready(result.map_err(Into::into))
    }
}

impl<T, E> AsyncResult<T, E> {
    /// Creates an `AsyncResult` from an immediately value.
    pub fn ready(result: Result<T, E>) -> Self {
        AsyncResult(AsyncResultKind::Ready(Some(result)))
    }

    /// Creates an `AsyncResult` from a closure representing an asynchronous computation.
    pub fn polling<F>(f: F) -> Self
    where
        F: FnMut(&mut Input<'_>) -> Poll<T, E> + Send + 'static,
    {
        AsyncResult(AsyncResultKind::Polling(Box::new(f)))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E> {
        match self.0 {
            AsyncResultKind::Ready(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            AsyncResultKind::Polling(ref mut f) => (f)(input),
        }
    }
}
