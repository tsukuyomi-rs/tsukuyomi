//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.4.0")]
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
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate tsukuyomi_macros;

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

#[cfg(feature = "use-native-tls")]
extern crate tokio_tls;

#[cfg(feature = "use-rustls")]
extern crate rustls;
#[cfg(feature = "use-rustls")]
extern crate tokio_rustls;

#[cfg(feature = "use-openssl")]
extern crate openssl;
#[cfg(feature = "use-openssl")]
extern crate tokio_openssl;

#[cfg(feature = "tower-middleware")]
extern crate tower_web;

#[cfg(test)]
extern crate matches;

mod common;

pub mod app;
pub mod error;
pub mod extractor;
pub mod fs;
pub mod handler;
pub mod input;
pub mod localmap;
pub mod output;
pub mod rt;
pub mod server;
pub mod test;

#[doc(inline)]
pub use crate::{
    app::App,
    common::{MaybeFuture, Never, NeverFuture, TryFrom},
    error::{
        Error, //
        HttpError,
        Result,
    },
    extractor::Extractor,
    handler::{
        Handler, //
        MakeHandler,
        ModifyHandler,
    },
    input::Input,
    output::{
        Output, //
        Responder,
    },
};
