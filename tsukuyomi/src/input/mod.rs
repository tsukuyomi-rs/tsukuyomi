//! Components for accessing HTTP requests and global/request-local data.

pub mod body;
pub mod local_map;

pub use {
    self::body::RequestBody,
    crate::app::imp::{
        is_set_current, //
        with_get_current,
        Cookies,
        Input,
        Params,
        State,
    },
};

use futures::{
    Future, //
    IntoFuture,
    Poll,
};

/// Creates a `Future` from the specified closure that process an abritrary asynchronous computation.
pub fn poll_fn<F, T, E>(mut f: F) -> impl Future<Item = T, Error = E>
where
    F: FnMut(&mut Input<'_>) -> Poll<T, E>,
{
    futures::future::poll_fn(move || with_get_current(|input| f(input)))
}

/// Creates a `Future` which has the same result as the future returned from the specified function.
pub fn lazy<F, R>(f: F) -> impl Future<Item = R::Item, Error = R::Error>
where
    F: FnOnce(&mut Input<'_>) -> R,
    R: IntoFuture,
{
    futures::future::lazy(move || with_get_current(f))
}
