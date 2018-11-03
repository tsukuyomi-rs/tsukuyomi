//! The definition of session backends

mod cookie;
mod redis;

pub use self::cookie::CookieSessionBackend;
pub use self::imp::Backend;
#[cfg(feature = "redis-backend")]
pub use self::redis::RedisSessionBackend;

pub(crate) mod imp {
    use futures::{Future, Poll};

    use tsukuyomi::error::Error;
    use tsukuyomi::input::Input;

    use crate::session::SessionState;

    pub trait ReadFuture {
        fn poll_read(&mut self, input: &mut Input<'_>) -> Poll<SessionState, Error>;
    }

    impl<F> ReadFuture for F
    where
        F: Future<Item = SessionState, Error = Error>,
    {
        #[inline]
        fn poll_read(&mut self, _: &mut Input<'_>) -> Poll<SessionState, Error> {
            self.poll()
        }
    }

    pub trait WriteFuture {
        fn poll_write(&mut self, input: &mut Input<'_>) -> Poll<(), Error>;
    }

    impl<F> WriteFuture for F
    where
        F: Future<Item = (), Error = Error>,
    {
        #[inline]
        fn poll_write(&mut self, _: &mut Input<'_>) -> Poll<(), Error> {
            self.poll()
        }
    }

    /// A trait representing the session backend.
    ///
    /// Currently the detailed trait definition is private.
    pub trait Backend: BackendImpl {}

    pub trait BackendImpl {
        type ReadFuture: ReadFuture + Send + 'static;
        type WriteFuture: WriteFuture + Send + 'static;

        fn read(&self, input: &mut Input<'_>) -> Self::ReadFuture;
        fn write(&self, input: &mut Input<'_>, values: SessionState) -> Self::WriteFuture;
    }
}
