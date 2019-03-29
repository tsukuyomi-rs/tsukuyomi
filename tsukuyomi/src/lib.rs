//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.6.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(test, deny(warnings))]
#![deny(clippy::unimplemented)]

#[macro_use]
pub mod util;

mod generic;
mod uri;

pub mod app;
pub mod endpoint;
pub mod error;
pub mod extractor;
pub mod fs;
pub mod future;
pub mod handler;
pub mod input;
pub mod output;
pub mod server;
pub mod test;
pub mod upgrade;

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
    output::{IntoResponse, Responder},
};

/// Re-export of crates used within the framework and frequently used on the user side.
pub mod vendor {
    pub use futures01 as futures;
    pub use http;
}
