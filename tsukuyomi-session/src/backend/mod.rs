//! The definition of session backends

mod cookie;
mod redis;

pub use self::cookie::CookieBackend;
#[cfg(feature = "use-redis")]
pub use self::redis::RedisBackend;
