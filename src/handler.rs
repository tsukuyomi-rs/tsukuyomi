//! `Handler` and supplemental components.

use futures::{Async, Future, Poll};
use std::fmt;
use std::sync::Arc;

use error::Error;
use input::{self, Input};
use output::{Output, Responder};

/// A trait representing handler functions.
pub trait Handler {
    /// Applies an incoming request to this handler.
    fn handle(&self, input: &mut Input) -> Handle;
}

impl<F> Handler for F
where
    F: Fn(&mut Input) -> Handle,
{
    #[inline]
    fn handle(&self, input: &mut Input) -> Handle {
        (*self)(input)
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input) -> Handle {
        (**self).handle(input)
    }
}

/// Create an instance of `Handler` from the provided function.
///
/// The provided handler is *synchronous*, which means that the provided handler
/// will return a result and immediately converted into an HTTP response without polling
/// the asynchronous status.
///
/// # Examples
///
/// ```
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::handler::ready_handler;
/// fn index(input: &mut Input) -> &'static str {
///     "Hello, Tsukuyomi.\n"
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/index.html", ready_handler(index)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
pub fn ready_handler<R>(f: impl Fn(&mut Input) -> R) -> impl Handler
where
    R: Responder,
{
    #[allow(missing_debug_implementations)]
    struct ReadyHandler<T>(T);

    impl<T, R> Handler for ReadyHandler<T>
    where
        T: Fn(&mut Input) -> R,
        R: Responder,
    {
        fn handle(&self, input: &mut Input) -> Handle {
            Handle::ready((self.0)(input).respond_to(input))
        }
    }

    ReadyHandler(f)
}

/// Create an instance of `Handler` from the provided function.
///
/// The provided handler is *asynchronous*, which means that the handler will
/// process some tasks by using the provided reference to `Input` and return a future for
/// processing the remaining task.
///
/// # Examples
///
/// ```
/// # extern crate futures;
/// # extern crate tsukuyomi;
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::error::Error;
/// # use tsukuyomi::input::Input;
/// # use futures::Future;
/// # use futures::future::lazy;
/// # use tsukuyomi::handler::async_handler;
/// fn handler(input: &mut Input)
///     -> impl Future<Item = String, Error = Error> + Send + 'static
/// {
///     let query = input.uri().query().unwrap_or("<empty>").to_owned();
///     lazy(move || {
///         Ok(format!("query = {}", query))
///     })
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/posts", async_handler(handler)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
pub fn async_handler<R>(f: impl Fn(&mut Input) -> R) -> impl Handler
where
    R: Future + Send + 'static,
    R::Item: Responder,
    Error: From<R::Error>,
{
    #[allow(missing_debug_implementations)]
    struct AsyncHandler<T>(T);

    impl<T, R> Handler for AsyncHandler<T>
    where
        T: Fn(&mut Input) -> R,
        R: Future + Send + 'static,
        R::Item: Responder,
        Error: From<R::Error>,
    {
        fn handle(&self, input: &mut Input) -> Handle {
            let mut future = (self.0)(input);
            Handle(HandleKind::Async(Box::new(move |input| {
                let item = try_ready!(input::with_set_current(input, || future.poll()));
                item.respond_to(input).map(Async::Ready)
            })))
        }
    }

    AsyncHandler(f)
}

/// Create an `Handler` from the provided function.
///
/// This function is equivalent to `async_handler(move |_| f())`.
#[inline(always)]
pub fn fully_async_handler<R>(f: impl Fn() -> R) -> impl Handler
where
    R: Future + Send + 'static,
    R::Item: Responder,
    Error: From<R::Error>,
{
    async_handler(move |_| f())
}

/// A type representing the return value from `Handler::handle`.
#[derive(Debug)]
pub struct Handle(HandleKind);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input) -> Poll<Output, Error> + Send>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for HandleKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle").finish()
    }
}

impl Handle {
    /// Creates a `Handle` from an HTTP response.
    pub fn ok(output: Output) -> Handle {
        Handle::ready(Ok(output))
    }

    /// Creates a `Handle` from an error value.
    pub fn err<E>(err: E) -> Handle
    where
        E: Into<Error>,
    {
        Handle::ready(Err(err.into()))
    }

    #[doc(hidden)]
    pub fn ready(result: Result<Output, Error>) -> Handle {
        Handle(HandleKind::Ready(Some(result)))
    }

    /// Creates a `Handle` from a future.
    pub fn wrap_future<F>(mut future: F) -> Handle
    where
        F: Future<Item = Output, Error = Error> + Send + 'static,
    {
        Handle(HandleKind::Async(Box::new(move |input| {
            input::with_set_current(input, || future.poll())
        })))
    }

    #[doc(hidden)]
    pub fn async_responder<F>(mut future: F) -> Handle
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
        Handle(HandleKind::Async(Box::new(move |input| {
            let x = try_ready!(input::with_set_current(input, || future.poll()));
            x.respond_to(input).map(Async::Ready)
        })))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Output, Error> {
        match self.0 {
            HandleKind::Ready(ref mut res) => res.take().expect("this future has already polled").map(Async::Ready),
            HandleKind::Async(ref mut f) => (f)(input),
        }
    }
}
