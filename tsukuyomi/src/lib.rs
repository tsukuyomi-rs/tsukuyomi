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

extern crate tsukuyomi_core as core;
extern crate tsukuyomi_server;

pub use crate::core::{app, error, extractor, handler, input, modifier, output};

#[allow(missing_docs)]
pub mod route {
    pub use crate::core::route::*;
    pub use crate::core::{connect, delete, get, head, options, patch, post, put, trace};
}

pub mod server {
    pub use tsukuyomi_server::*;
}
