//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.3.2")]
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

pub extern crate tsukuyomi_server as server;

extern crate bytes;
extern crate cookie;
extern crate either;
extern crate failure;
extern crate filetime;
extern crate futures;
extern crate http;
#[cfg_attr(test, macro_use)]
extern crate indexmap;
extern crate log;
extern crate mime;
extern crate serde;
extern crate serde_json;
extern crate time;
extern crate tower_service;
extern crate walkdir;

#[cfg(test)]
extern crate matches;

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

pub mod app;
pub mod contrib;
pub mod error;
pub mod handler;
pub mod input;
pub mod modifier;
pub mod output;
pub(crate) mod recognizer;
