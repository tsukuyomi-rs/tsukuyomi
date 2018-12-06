//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.4.0")]
#![allow(clippy::stutter)]
#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]

mod common;
mod generic;

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
    common::{Chain, MaybeFuture, Never, NeverFuture, TryFrom},
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
