#![doc(html_root_url = "https://docs.rs/tsukuyomi-server/0.2.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

mod error;

pub mod rt;
pub mod server;
pub mod test;

pub use crate::{
    error::{Error, Result},
    server::Server,
};

type CritError = Box<dyn std::error::Error + Send + Sync + 'static>;
