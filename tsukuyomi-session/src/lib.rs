//! Modifiers for supporting session management.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-session/0.1.0")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate cookie;
extern crate futures;
extern crate serde;
extern crate serde_json;
extern crate time;
extern crate tsukuyomi;

#[cfg(feature = "redis-backend")]
extern crate redis;
#[cfg(feature = "redis-backend")]
extern crate uuid;

pub mod backend;
mod session;
mod storage;
mod util;

pub use crate::{
    session::{extractor, Session},
    storage::SessionStorage,
};

#[allow(missing_docs)]
pub fn storage<B>(backend: B) -> SessionStorage<B>
where
    B: crate::backend::Backend,
{
    SessionStorage::new(backend)
}
