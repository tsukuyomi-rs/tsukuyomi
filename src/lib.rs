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

extern crate tsukuyomi_internal as internal;

extern crate bytes;
extern crate failure;
extern crate filetime;
extern crate futures;
extern crate http;
extern crate log;
extern crate mime;
extern crate serde;
extern crate serde_json;
extern crate time;
extern crate walkdir;

pub use crate::internal::{app, error, extractor, handler, input, modifier, output, server};

#[allow(missing_docs)]
pub mod route {
    pub use crate::internal::route::*;
    pub use crate::internal::{connect, delete, get, head, options, patch, post, put, trace};
}

pub mod askama;
pub mod fs;
pub mod json;
pub mod websocket;
