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
    Result(Option<Result<T, E>>),
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
            AsyncResultKind::Result(ref res) => f.debug_tuple("Result").field(res).finish(),
            AsyncResultKind::Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl<T, E> From<Result<T, E>> for AsyncResult<T, E>
where
    Error: From<E>,
{
    fn from(result: Result<T, E>) -> Self {
        Self::result(result.map_err(Into::into))
    }
}

impl<T, E> AsyncResult<T, E> {
    /// Creates an `AsyncResult` from an immediately value.
    pub fn ok(output: T) -> Self {
        Self::result(Ok(output))
    }

    /// Creates an `AsyncResult` from an immediately value.
    pub fn err(err: E) -> Self {
        Self::result(Err(err))
    }

    /// Creates an `AsyncResult` from an immediately value.
    pub fn result(result: Result<T, E>) -> Self {
        AsyncResult(AsyncResultKind::Result(Some(result)))
    }

    pub fn ready<F>(f: F) -> Self
    where
        F: FnOnce(&mut Input<'_>) -> Result<T, E> + Send + 'static,
    {
        let mut f = Some(f);
        Self::polling(move |input| {
            (f.take().expect("the future has already polled"))(input).map(Async::Ready)
        })
    }

    /// Creates an `AsyncResult` from a closure representing an asynchronous computation.
    pub fn polling<F>(f: F) -> Self
    where
        F: FnMut(&mut Input<'_>) -> Poll<T, E> + Send + 'static,
    {
        AsyncResult(AsyncResultKind::Polling(Box::new(f)))
    }

    pub fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E> {
        match self.0 {
            AsyncResultKind::Result(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            AsyncResultKind::Polling(ref mut f) => (f)(input),
        }
    }
}
