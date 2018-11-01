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

#[cfg(feature = "websocket")]
extern crate base64;
#[cfg(feature = "websocket")]
extern crate sha1;
#[cfg(feature = "websocket")]
extern crate tokio_tungstenite;
#[cfg(feature = "websocket")]
extern crate tungstenite;

#[cfg(feature = "askama")]
extern crate askama;
#[cfg(feature = "askama")]
extern crate mime_guess;

pub use crate::internal::{app, error, extractor, handler, input, modifier, output, server};
pub mod contrib;

#[allow(missing_docs)]
pub mod route {
    pub use crate::internal::route::*;
    pub use crate::internal::{connect, delete, get, head, options, patch, post, put, trace};
}
