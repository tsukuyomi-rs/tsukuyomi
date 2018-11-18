//! Primitives and re-exports for handling asynchronous tasks.

#[doc(no_inline)]
pub use {
    futures::{sync::oneshot::SpawnHandle, Async, Future, Poll},
    tokio::executor::{spawn, DefaultExecutor, Executor, Spawn, SpawnError},
    tokio_threadpool::{blocking as poll_blocking, BlockingError},
};

/// Spawns the specified `Future` onto the default task executor, and returns its handle.
#[inline]
pub fn spawn_with_handle<F>(future: F) -> SpawnHandle<F::Item, F::Error>
where
    F: Future + Send + 'static,
    F::Item: Send + 'static,
    F::Error: Send + 'static,
{
    futures::sync::oneshot::spawn(future, &DefaultExecutor::current())
}

/// Creates a `Future` to execute the specified function that will block the current thread.
///
/// The future genereted by this function internally calls the Tokio's blocking API,
/// and then enters a blocking section after other tasks are moved to another thread.
/// See [the documentation of `tokio_threadpool::blocking`][blocking] for details.
///
/// [blocking]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool/fn.blocking.html
pub fn blocking<T>(op: impl FnOnce() -> T) -> impl Future<Item = T, Error = BlockingError> {
    let mut op = Some(op);
    futures::future::poll_fn(move || {
        poll_blocking(|| {
            let op = op.take().expect("The future has already polled");
            op()
        })
    })
}

/// Spawns a task to execute the specified blocking section and returns its handle.
///
/// This function is equivalent to `spawn_with_handle(blocking(op))`.
#[inline]
pub fn spawn_fn<T>(op: impl FnOnce() -> T + Send + 'static) -> SpawnHandle<T, BlockingError>
where
    T: Send + 'static,
{
    spawn_with_handle(blocking(op))
}
