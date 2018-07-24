//! `Handler` and supplemental components.

use futures::{Async, Future, IntoFuture};
use std::sync::Arc;

use error::Error;
use input::{self, Input};
use output::{AsyncResponder, Respond, Responder};

/// A trait representing handler functions.
pub trait Handler {
    /// Applies an incoming request to this handler.
    fn handle(&self, input: &mut Input) -> Respond;
}

impl<F, T> Handler for F
where
    F: Fn(&mut Input) -> T,
    T: AsyncResponder,
{
    #[inline]
    fn handle(&self, input: &mut Input) -> Respond {
        (*self)(input).respond_to(input)
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input) -> Respond {
        (**self).handle(input)
    }
}

/// A helper function to instantiate a `Handler` from an async function.
///
/// # Examples
///
/// ```
/// # extern crate tsukuyomi;
/// # extern crate futures;
/// # use futures::prelude::*;
/// # use tsukuyomi::{App, Input, Error};
/// # use tsukuyomi::handler::wrap_async;
/// #
/// fn handler(input: &mut Input)
///     -> impl Future<Item = String, Error = Error> + Send + 'static
/// {
///     input.body_mut()
///         .read_all()
///         .convert_to()
/// }
///
/// # fn main() -> tsukuyomi::AppResult<()> {
/// let app = App::builder()
///     .route(("/", wrap_async(handler)))
///     .finish()?;
/// # Ok(())
/// # }
/// ```
pub fn wrap_async<F>(f: impl Fn(&mut Input) -> F) -> impl Handler
where
    F: IntoFuture,
    F::Future: Send + 'static,
    F::Item: Responder,
    Error: From<F::Error>,
{
    move |input: &mut Input| {
        let mut future = f(input).into_future();
        Respond::new(move |input| {
            let item = try_ready!(input::with_set_current(input, || future.poll()));
            Responder::respond_to(item, input).map(Async::Ready)
        })
    }
}
