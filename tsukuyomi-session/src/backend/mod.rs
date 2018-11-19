//! The definition of session backends

mod cookie;
mod redis;

#[cfg(feature = "redis-backend")]
pub use self::redis::RedisSessionBackend;
pub use self::{cookie::CookieSessionBackend, imp::Backend};

pub(crate) mod imp {
    use {crate::session::SessionInner, tsukuyomi::AsyncResult};

    /// A trait representing the session backend.
    ///
    /// Currently the detailed trait definition is private.
    pub trait Backend: BackendImpl {}

    pub trait BackendImpl {
        fn read(&self) -> AsyncResult<SessionInner>;
        fn write(&self, inner: SessionInner) -> AsyncResult<()>;
    }
}
