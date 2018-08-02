//! `Handler` and supplemental components.

use futures::{Async, Poll};
use std::fmt;
use std::sync::Arc;

use error::Error;
use input::Input;
use output::{AsyncResponder, Output, Responder};

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

/// A type representing the return value from `Handler::handle`.
pub struct Handle(HandleKind);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle").finish()
    }
}

impl Handle {
    #[doc(hidden)]
    pub fn ready(result: Result<Output, Error>) -> Handle {
        Handle(HandleKind::Ready(Some(result)))
    }

    #[doc(hidden)]
    pub fn wrap_async(mut x: impl AsyncResponder) -> Handle {
        Handle(HandleKind::Async(Box::new(move |input| {
            x.poll_respond_to(input)
        })))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Output, Error> {
        match self.0 {
            HandleKind::Ready(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            HandleKind::Async(ref mut f) => (f)(input),
        }
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
/// # use tsukuyomi::handler::wrap_ready;
/// fn index(input: &mut Input) -> &'static str {
///     "Hello, Tsukuyomi.\n"
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/index.html", wrap_ready(index)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
pub fn wrap_ready<R>(f: impl Fn(&mut Input) -> R) -> impl Handler
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
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::error::Error;
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::output::AsyncResponder;
/// # use tsukuyomi::handler::wrap_async;
/// fn handler(input: &mut Input) -> impl AsyncResponder<Output = String> {
///     input.body_mut().read_all().convert_to()
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/posts", wrap_async(handler)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
///
/// ```ignore
/// # extern crate tsukuyomi;
/// # extern crate futures_await as futures;
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::error::Error;
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::output::Responder;
/// # use tsukuyomi::handler::wrap_async;
/// # use futures::prelude::*;
/// #[async]
/// fn handler() -> tsukuyomi::Result<impl Responder> {
///     Ok("Hello")
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/posts", wrap_async(handler)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
pub fn wrap_async<R>(f: impl Fn(&mut Input) -> R) -> impl Handler
where
    R: AsyncResponder,
{
    #[allow(missing_debug_implementations)]
    struct AsyncHandler<T>(T);

    impl<T, R> Handler for AsyncHandler<T>
    where
        T: Fn(&mut Input) -> R,
        R: AsyncResponder,
    {
        fn handle(&self, input: &mut Input) -> Handle {
            Handle::wrap_async((self.0)(input))
        }
    }

    AsyncHandler(f)
}
