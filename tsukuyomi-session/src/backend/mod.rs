//! The definition of session backends

mod cookie;
mod redis;

#[cfg(feature = "redis-backend")]
pub use self::redis::RedisSessionBackend;
pub use self::{cookie::CookieSessionBackend, imp::Backend};

pub(crate) mod imp {
    use {
        crate::session::SessionInner,
        futures::{Future, Poll},
        tsukuyomi::{error::Error, input::Input},
    };

    pub trait ReadFuture {
        fn poll_read(&mut self, input: &mut Input<'_>) -> Poll<SessionInner, Error>;
    }

    impl<F> ReadFuture for F
    where
        F: Future<Item = SessionInner, Error = Error>,
    {
        #[inline]
        fn poll_read(&mut self, _: &mut Input<'_>) -> Poll<SessionInner, Error> {
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
        fn write(&self, input: &mut Input<'_>, values: SessionInner) -> Self::WriteFuture;
    }
}
