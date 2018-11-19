//! Definition of `Handler` and `Modifier`.

use {
    crate::{
        error::Error, //
        input::Input,
    },
    futures::{
        Async, //
        Future,
        IntoFuture,
        Poll,
    },
    std::fmt,
};

/// A type representing asynchronous computation in Tsukuyomi.
pub struct AsyncResult<T, E = Error> {
    kind: AsyncResultKind<T, E>,
}

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
        match self.kind {
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
    /// Creates an `AsyncResult` from the specified `Result`.
    pub fn result(result: Result<T, E>) -> Self {
        Self {
            kind: AsyncResultKind::Result(Some(result)),
        }
    }

    /// Creates an `AsyncResult` from a closure representing an asynchronous computation.
    pub fn poll_fn<F>(f: F) -> Self
    where
        F: FnMut(&mut Input<'_>) -> Poll<T, E> + Send + 'static,
    {
        Self {
            kind: AsyncResultKind::Polling(Box::new(f)),
        }
    }

    /// Progress the inner asynchronous computation with the specified `Input`.
    pub fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E> {
        match self.kind {
            AsyncResultKind::Result(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            AsyncResultKind::Polling(ref mut f) => (f)(input),
        }
    }
}

impl<T, E> AsyncResult<T, E> {
    /// Creates an `AsyncResult` from an successful value.
    pub fn ok(output: T) -> Self {
        Self::result(Ok(output))
    }

    /// Creates an `AsyncResult` from an error value.
    pub fn err(err: E) -> Self {
        Self::result(Err(err))
    }

    /// Creates an `AsyncResult` from a closure which will returns a `Result` immediately.
    pub fn ready<F>(f: F) -> Self
    where
        F: FnOnce(&mut Input<'_>) -> Result<T, E> + Send + 'static,
    {
        let mut f = Some(f);
        Self::poll_fn(move |input| {
            (f.take().expect("the future has already polled"))(input).map(Async::Ready)
        })
    }

    /// Creates an `AsyncResult` from a closure which will returns a `Future`.
    pub fn lazy<F, R>(f: F) -> Self
    where
        F: FnOnce(&mut Input<'_>) -> R + Send + 'static,
        R: IntoFuture<Item = T, Error = E>,
        R::Future: Send + 'static,
    {
        #[allow(missing_debug_implementations)]
        enum State<F, T> {
            Init(F),
            Pending(T),
            Gone,
        }

        let mut state: State<F, R::Future> = State::Init(f);

        Self::poll_fn(move |input| loop {
            state = match std::mem::replace(&mut state, State::Gone) {
                State::Init(f) => State::Pending(f(input).into_future()),
                State::Pending(ref mut future) => return input.with_set_current(|| future.poll()),
                State::Gone => panic!("the future has already polled"),
            };
        })
    }
}
