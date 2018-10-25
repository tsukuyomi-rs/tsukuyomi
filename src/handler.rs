//! `Handler` and supplemental components.

use either::Either;
use futures::{Async, Future, Poll};
use std::fmt;
use std::sync::Arc;

use crate::error::Error;
use crate::input::Input;
use crate::output::{Output, Responder};

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
    pub fn ready(result: Result<Output, Error>) -> Self {
        Handle(HandleKind::Ready(Some(result)))
    }

    #[doc(hidden)]
    pub fn wrap_async<F>(mut x: F) -> Self
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
        Handle(HandleKind::Async(Box::new(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || x.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
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
/// # extern crate futures;
/// # extern crate tsukuyomi;
/// # use futures::prelude::*;
/// # use tsukuyomi::app::App;
/// # use tsukuyomi::input::Input;
/// # use tsukuyomi::error::Error;
/// # use tsukuyomi::input::body::Plain;
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
}

#[macro_export]
macro_rules! handler {
    ($vis:vis fn $name:ident () -> $ret:ty {
        $($bd:stmt),*
    }) => {
        $vis fn $name(input: &mut $crate::input::Input<'_>) -> $crate::handler::Handle {
            fn inner(_: ()) -> $ret {
                $($bd)*
            }
            {
                use $crate::handler::private::Future;
                $crate::handler::Handle::wrap_async(
                    input.extract::<()>().and_then(inner)
                )
            }
        }
    };

    ($vis:vis fn $name:ident ($( $arg:ident : $t:ty ),+) -> $ret:ty {
        $($bd:stmt),*
    }) => {
        $vis fn $name(input: &mut $crate::input::Input<'_>) -> $crate::handler::Handle {
            fn inner( ($($arg,)+) : ($($t,)+) ) -> $ret {
                $($bd)*
            }
            {
                use $crate::handler::private::Future;
                $crate::handler::Handle::wrap_async(
                    input.extract::<($($t,)+)>()
                        .and_then(inner)
                )
            }
        }
    };
}
