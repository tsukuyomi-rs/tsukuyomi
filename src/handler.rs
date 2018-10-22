//! `Handler` and supplemental components.

use either::Either;
use futures::{Async, Poll};
use std::fmt;
use std::sync::Arc;

use crate::error::Error;
use crate::input::Input;
use crate::output::{AsyncResponder, Output, Responder};

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

/// A helper function which creats an instance of `Handler` for use as a placeholder.
pub fn unimplemented() -> impl Handler {
    self::wrap_ready(|_| {
        Err::<(), crate::error::Error>(
            crate::error::Failure::internal_server_error(failure::format_err!(
                "not implemented yet"
            )).into(),
        )
    })
}

/// A type representing the return value from `Handler::handle`.
pub struct Handle(HandleKind);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    pub(crate) fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Output, Error> {
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
/// # #[allow(unused_variables)]
/// fn index(input: &mut Input) -> &'static str {
///     "Hello, Tsukuyomi.\n"
/// }
///
/// # fn main() -> tsukuyomi::app::AppResult<()> {
/// let app = App::builder()
///     .route(("/index.html", wrap_ready(index)))
///     .finish()?;
/// # drop(app);
/// # Ok(())
/// # }
/// ```
pub fn wrap_ready<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: Responder,
{
    #[allow(missing_debug_implementations)]
    struct ReadyHandler<T>(T);

    impl<T, R> Handler for ReadyHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: Responder,
    {
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::ready(
                (self.0)(input)
                    .respond_to(input)
                    .map(|res| res.map(Into::into))
                    .map_err(Into::into),
            )
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
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::output::AsyncResponder;
/// # use tsukuyomi::handler::wrap_async;
/// fn handler(input: &mut Input) -> impl AsyncResponder<Output = String> {
///     input.body_mut().read_all().convert_to()
/// }
///
/// # fn main() -> tsukuyomi::app::AppResult<()> {
/// let app = App::builder()
///     .route(("/posts", wrap_async(handler)))
///     .finish()?;
/// # drop(app);
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
pub fn wrap_async<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: AsyncResponder,
{
    #[allow(missing_debug_implementations)]
    struct AsyncHandler<T>(T);

    impl<T, R> Handler for AsyncHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: AsyncResponder,
    {
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::wrap_async((self.0)(input))
        }
    }

    AsyncHandler(f)
}
