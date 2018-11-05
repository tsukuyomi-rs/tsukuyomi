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

extern crate tsukuyomi_core;
extern crate tsukuyomi_server;

pub use tsukuyomi_core::{app, error, extractor, handler, input, modifier, output};
pub use tsukuyomi_server::{local, rt, server, service};

#[allow(missing_docs)]
pub mod route {
    pub use tsukuyomi_core::route::*;
    pub use tsukuyomi_core::{connect, delete, get, head, options, patch, post, put, trace};
}

#[allow(missing_docs)]
pub fn launch(app: app::App) -> server::Server<app::App> {
    server::Server::new(app)
}
