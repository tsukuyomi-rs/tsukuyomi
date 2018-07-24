//! `Handler` and supplemental components.

use std::sync::Arc;

use input::Input;
use output::responder::IntoResponder;
use output::{AsyncResponder, Respond};

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

/// A helper function to instantiate a `Handler` from a function which will return a `Future`.
pub fn wrap_async<R>(f: impl Fn(&mut Input) -> R) -> impl Handler
where
    R: IntoResponder,
{
    move |input: &mut Input| f(input).into_responder()
}
