#![allow(missing_docs)]

use futures::{Async, Poll};
use std::cell::Cell;
use std::{error, fmt};
use tokio_threadpool;

#[derive(Debug, Copy, Clone)]
pub enum RuntimeMode {
    ThreadPool,
    CurrentThread,
}

thread_local!(static MODE: Cell<Option<RuntimeMode>> = Cell::new(None));

struct ResetOnDrop(Option<RuntimeMode>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        MODE.with(|mode| mode.set(self.0));
    }
}

pub(crate) fn with_set_mode<R>(mode: RuntimeMode, f: impl FnOnce() -> R) -> R {
    let prev = MODE.with(|m| m.replace(Some(mode)));
    let _reset = ResetOnDrop(prev);
    if prev.is_some() {
        panic!("The runtime mode has already set.");
    }
    f()
}

pub fn current_mode() -> Option<RuntimeMode> {
    MODE.with(|mode| mode.get())
}

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
