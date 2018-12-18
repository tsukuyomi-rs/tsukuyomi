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
pub mod util;

mod generic;
mod uri;

pub mod app;
pub mod config;
pub mod endpoint;
pub mod error;
pub mod extractor;
pub mod fs;
pub mod future;
pub mod handler;
pub mod input;
pub mod modifiers;
pub mod output;
pub mod responder;
pub mod rt;
pub mod server;
pub mod test;

#[doc(inline)]
pub use crate::{
    app::App,
    endpoint::Endpoint,
    error::{
        Error, //
        HttpError,
        Result,
    },
    extractor::Extractor,
    handler::{Handler, ModifyHandler},
    input::Input,
    output::IntoResponse,
    responder::Responder,
    server::Server,
};

/// Re-export of crates used within the framework and frequently used on the user side.
pub mod vendor {
    pub use futures01 as futures;
    pub use http;
}
