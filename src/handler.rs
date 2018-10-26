//! `Handler` and supplemental components.

use either::Either;
use futures::{Async, Future, Poll};
use std::fmt;
use std::sync::Arc;

use crate::error::Error;
use crate::input::Input;
use crate::output::{Output, Responder};

pub use crate::codegen::handler;

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

    #[doc(hidden)]
    #[deprecated(
        since = "0.3.3",
        note = "This method will remove in the future version"
    )]
    #[inline]
    pub fn wrap_async<F>(mut x: F) -> Self
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
        Self::polling(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || x.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
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
/// # extern crate futures;
/// # extern crate tsukuyomi;
/// # use futures::prelude::*;
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::error::Error;
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::extract::body::Plain;
/// # use tsukuyomi::handler::wrap_async;
/// fn handler(input: &mut Input) -> impl Future<Error = Error, Item = String> {
///     input.extract::<Plain>().map(Plain::into_inner)
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
pub fn wrap_async<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: Future + Send + 'static,
    R::Item: Responder,
    Error: From<R::Error>,
{
    #[allow(missing_debug_implementations)]
    struct AsyncHandler<T>(T);

    impl<T, R> Handler for AsyncHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: Future + Send + 'static,
        R::Item: Responder,
        Error: From<R::Error>,
    {
        #[allow(deprecated)]
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::wrap_async((self.0)(input))
        }
    }

    AsyncHandler(f)
}

// not a public API.
#[doc(hidden)]
pub mod private {
    pub use futures::Future;
    use futures::{Async, IntoFuture};

    use super::Handle;
    use crate::error::Error;
    use crate::extract::FromInput;
    use crate::input::Input;
    use crate::output::Responder;

    pub fn handle_ready<F, T, R>(input: &mut Input<'_>, f: F) -> Handle
    where
        F: FnOnce(T) -> R + Send + 'static,
        T: FromInput,
        T: Send + 'static,
        T::Ctx: Send + 'static,
        R: Responder,
    {
        let mut future = input.extract::<T>().map(f);
        Handle::polling(move |input| {
            futures::try_ready!(future.poll())
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
    }

    pub fn handle_async<F, T, R>(input: &mut Input<'_>, f: F) -> Handle
    where
        F: FnOnce(T) -> R + Send + 'static,
        T: FromInput,
        T: Send + 'static,
        T::Ctx: Send + 'static,
        R: IntoFuture<Error = Error> + 'static,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        let mut future = input.extract::<T>().and_then(f);
        Handle::polling(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
    }
}
