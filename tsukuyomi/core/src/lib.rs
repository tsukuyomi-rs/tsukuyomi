//! The implementation crate for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-core/0.4.0-dev")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![doc(test(no_crate_inject))]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]

extern crate bytes;
extern crate cookie;
extern crate either;
extern crate failure;
extern crate futures;
extern crate http;
#[cfg_attr(test, macro_use)]
extern crate indexmap;
extern crate mime;
extern crate serde;
extern crate tower_service;
extern crate url;
extern crate uuid;

extern crate tsukuyomi_macros as macros;
extern crate tsukuyomi_server as server;

#[doc(hidden)]
pub use crate::macros::*;

#[cfg(test)]
extern crate matches;

pub mod app;
pub mod error;
pub mod extractor;
pub mod handler;
pub mod input;
pub mod modifier;
pub mod output;
mod recognizer;
pub mod route;
