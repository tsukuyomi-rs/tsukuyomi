//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.4.0-dev")]
#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]

extern crate tsukuyomi_internal;
extern crate tsukuyomi_macros;

#[doc(hidden)]
pub use tsukuyomi_macros::{route_expr_impl, validate_prefix};

extern crate bytes;
extern crate cookie;
extern crate either;
extern crate failure;
extern crate filetime;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate indexmap;
extern crate log;
extern crate mime;
extern crate serde;
extern crate time;
extern crate tokio;
extern crate tokio_threadpool;
extern crate tower_service;
extern crate url;
extern crate uuid;

#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

#[cfg(test)]
extern crate matches;

#[macro_use]
#[doc(hidden)]
pub mod macros;

mod recognizer;
mod scoped_map;
use tsukuyomi_internal::uri;

pub mod app;
pub mod error;
pub mod extractor;
pub mod fs;
pub mod input;
pub mod output;
pub mod rt;
pub mod server;
pub mod test;

pub fn app() -> crate::app::Builder<(), ()> {
    crate::app::Builder::default()
}

pub use self::server::imp::server;
