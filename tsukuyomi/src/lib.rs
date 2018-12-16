//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.5.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

#[macro_use]
pub mod core;

mod generic;
mod uri;

pub mod app;
pub mod config;
pub mod endpoint;
pub mod error;
pub mod extractor;
pub mod fs;
pub mod handler;
pub mod input;
pub mod output;
pub mod rt;
pub mod server;
pub mod service;
pub mod test;

#[doc(inline)]
pub use crate::{
    app::App,
    error::{
        Error, //
        HttpError,
        Result,
    },
    extractor::Extractor,
    handler::Handler,
    input::Input,
    output::{
        IntoResponse, //
        Responder,
    },
    server::Server,
};
