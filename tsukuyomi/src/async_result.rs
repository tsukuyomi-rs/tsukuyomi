//! Definition of `Handler` and `Modifier`.

use {
    crate::{error::Error, input::Input},
    futures::{Async, Poll},
    std::fmt,
};

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
