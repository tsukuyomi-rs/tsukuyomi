use futures::{Async, Poll};
use std::{error, fmt};
use tokio_threadpool;

use super::runtime::{current_mode, RuntimeMode};

/// A error type which will be returned from `blocking`.
#[derive(Debug)]
pub struct BlockingError(tokio_threadpool::BlockingError);

impl BlockingError {
    pub fn into_inner(self) -> tokio_threadpool::BlockingError {
        self.0
    }
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Display for BlockingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg_attr(tarpaulin, skip)]
impl error::Error for BlockingError {
    fn description(&self) -> &str {
        self.0.description()
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        self.0.cause()
    }
}

/// Enter a blocking section of code if available.
///
/// This function is a wrapper of `tokio_threadpool::blocking`.
/// If the current runtime is a current_thread, this function does not call the original
/// blocking API and immediately block the curren thread with the provided function.
///
/// See also the documentation of [`tokio_threadpool::blocking`].
///
/// [`tokio_threadpool::blocking`]: https://docs.rs/tokio-threadpool/0.1/tokio_threadpool/fn.blocking.html
pub fn blocking<R>(f: impl FnOnce() -> R) -> Poll<R, BlockingError> {
    match current_mode() {
        Some(RuntimeMode::ThreadPool) | None => tokio_threadpool::blocking(f).map_err(BlockingError),
        Some(RuntimeMode::CurrentThread) => Ok(Async::Ready(f())),
    }
}
