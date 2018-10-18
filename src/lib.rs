// FIXME: remove this feature gate as soon as the rustc version used in docs.rs is updated
#![cfg_attr(tsukuyomi_inject_extern_prelude, feature(extern_prelude))]

//! Tsukuyomi is a next generation Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.2.2")]
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

extern crate tsukuyomi_server as server;

extern crate bytes;
extern crate cookie;
extern crate either;
#[macro_use]
extern crate failure;
extern crate filetime;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
#[cfg_attr(test, macro_use)]
extern crate indexmap;
#[macro_use]
extern crate log;
extern crate mime;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate time;
extern crate tokio_io;
#[cfg(test)]
#[macro_use]
extern crate matches;

#[cfg(feature = "websocket")]
extern crate base64;
#[cfg(feature = "websocket")]
extern crate sha1;
#[cfg(feature = "websocket")]
extern crate tokio_tungstenite;
#[cfg(feature = "websocket")]
extern crate tungstenite;

pub mod app;
pub mod error;
pub mod fs;
pub mod handler;
pub mod input;
pub mod json;
pub mod modifier;
pub mod output;
pub(crate) mod recognizer;
#[cfg(feature = "websocket")]
pub mod websocket;
