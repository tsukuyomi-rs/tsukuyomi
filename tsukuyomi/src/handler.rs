//! Definition of `Handler`.

use {
    crate::{
        error::Error, //
        input::Input,
        output::Output,
    },
    either::Either,
    futures::{Async, Future, IntoFuture, Poll},
    std::{fmt, sync::Arc},
};

pub trait AsyncResult<T, E = Error> {
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E>;
}

impl<L, R, T, E> AsyncResult<T, E> for Either<L, R>
where
    L: AsyncResult<T, E>,
    R: AsyncResult<T, E>,
{
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E> {
        match self {
            Either::Left(l) => l.poll_ready(input),
            Either::Right(r) => r.poll_ready(input),
        }
    }
}

pub fn poll_fn<T, E>(f: impl FnMut(&mut Input<'_>) -> Poll<T, E>) -> impl AsyncResult<T, E> {
    #[allow(missing_debug_implementations)]
    struct PollFn<F>(F);

    impl<F, T, E> AsyncResult<T, E> for PollFn<F>
    where
        F: FnMut(&mut Input<'_>) -> Poll<T, E>,
    {
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, E> {
            (self.0)(input)
        }
    }

    PollFn(f)
}

pub fn future<F>(future: F) -> impl AsyncResult<F::Item, F::Error>
where
    F: IntoFuture,
{
    let mut future = future.into_future();
    poll_fn(move |input| input.with_set_current(|| future.poll()))
}

/// Creates an `AsyncResult` from the specified `Result`.
pub fn result<T, E>(result: Result<T, E>) -> impl AsyncResult<T, E> {
    #[allow(missing_debug_implementations)]
    struct AsyncResultValue<T, E>(Option<Result<T, E>>);

    impl<T, E> AsyncResult<T, E> for AsyncResultValue<T, E> {
        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<T, E> {
            self.0.take().unwrap().map(Async::Ready)
        }
    }

    AsyncResultValue(Some(result))
}

/// Creates an `AsyncResult` from an successful value.
pub fn ok<T, E>(output: T) -> impl AsyncResult<T, E> {
    self::result(Ok(output))
}

/// Creates an `AsyncResult` from an error value.
pub fn err<T, E>(err: E) -> impl AsyncResult<T, E> {
    self::result(Err(err))
}

/// Creates an `AsyncResult` from a closure which will returns a `Result` immediately.
pub fn ready<T, E>(f: impl FnOnce(&mut Input<'_>) -> Result<T, E>) -> impl AsyncResult<T, E> {
    let mut f = Some(f);
    self::poll_fn(move |input| {
        (f.take().expect("the future has already polled"))(input).map(Async::Ready)
    })
}

/// Creates an `AsyncResult` from a closure which will returns a `Future`.
pub fn lazy<T, E, F, R>(f: F) -> impl AsyncResult<T, E>
where
    F: FnOnce(&mut Input<'_>) -> R,
    R: IntoFuture<Item = T, Error = E>,
{
    #[allow(missing_debug_implementations)]
    enum State<F, T> {
        Init(F),
        Pending(T),
        Gone,
    }

    let mut state: State<F, R::Future> = State::Init(f);

    self::poll_fn(move |input| loop {
        state = match std::mem::replace(&mut state, State::Gone) {
            State::Init(f) => State::Pending(f(input).into_future()),
            State::Pending(ref mut future) => return input.with_set_current(|| future.poll()),
            State::Gone => panic!("the future has already polled"),
        };
    })
}

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler: Send + Sync + 'static {
    type Handle: AsyncResult<Output> + Send + 'static;

    /// Creates an `AsyncResult` which handles the incoming request.
    fn handle(&self) -> Self::Handle;
}

impl<F, R> Handler for F
where
    F: Fn() -> R + Send + Sync + 'static,
    R: AsyncResult<Output> + Send + 'static,
{
    type Handle = R;

    #[inline]
    fn handle(&self) -> Self::Handle {
        (*self)()
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    type Handle = H::Handle;

    #[inline]
    fn handle(&self) -> Self::Handle {
        (**self).handle()
    }
}

impl<L, R> Handler for Either<L, R>
where
    L: Handler,
    R: Handler,
{
    type Handle = Either<L::Handle, R::Handle>;

    #[inline]
    fn handle(&self) -> Self::Handle {
        match self {
            Either::Left(ref handler) => Either::Left(handler.handle()),
            Either::Right(ref handler) => Either::Right(handler.handle()),
        }
    }
}

pub fn raw<F, R>(f: F) -> impl Handler
where
    F: Fn() -> R + Send + Sync + 'static,
    R: AsyncResult<Output> + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Handler for Raw<F>
    where
        F: Fn() -> R + Send + Sync + 'static,
        R: AsyncResult<Output> + Send + 'static,
    {
        type Handle = R;

        #[inline]
        fn handle(&self) -> Self::Handle {
            (self.0)()
        }
    }

    Raw(f)
}

pub(crate) struct BoxedHandler(
    Box<dyn Fn() -> Box<dyn AsyncResult<Output> + Send + 'static> + Send + Sync + 'static>,
);

impl fmt::Debug for BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> From<H> for BoxedHandler
where
    H: Handler,
{
    fn from(handler: H) -> Self {
        BoxedHandler(Box::new(move || {
            Box::new(handler.handle()) as Box<dyn AsyncResult<Output> + Send + 'static>
        }))
    }
}

impl BoxedHandler {
    pub(crate) fn handle(&self) -> Box<dyn AsyncResult<Output> + Send + 'static> {
        (self.0)()
    }
}
