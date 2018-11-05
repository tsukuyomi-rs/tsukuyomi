//! A general purpose HTTP server based on Hyper and tower-service.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-server/0.4.0-dev")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]

extern crate bytes;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate tokio;
extern crate tokio_threadpool;
extern crate tower_service;

#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

pub mod local;
pub mod rt;
pub mod server;
pub mod service;
