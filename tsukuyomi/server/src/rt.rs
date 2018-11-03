//! Primitives for spawning asynchronous tasks

#[doc(no_inline)]
pub use futures::sync::oneshot::SpawnHandle;
#[doc(no_inline)]
pub use tokio::executor::{spawn, DefaultExecutor, Executor, Spawn, SpawnError};
#[doc(no_inline)]
pub use tokio::runtime::run;
#[doc(no_inline)]
pub use tokio_threadpool::{blocking, BlockingError};

use futures::sync::oneshot;
use futures::Future;

/// Spawns a future onto the default executor and returns its handle.
#[inline]
pub fn spawn_with_handle<F>(future: F) -> SpawnHandle<F::Item, F::Error>
where
    F: Future + Send + 'static,
    F::Item: Send + 'static,
    F::Error: Send + 'static,
{
    oneshot::spawn(future, &DefaultExecutor::current())
}
