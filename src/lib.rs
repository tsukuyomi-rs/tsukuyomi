//! Tsukuyomi is a next generation Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.2.1/tsukuyomi")]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(unreachable_pub)]
#![deny(unused_extern_crates)]
#![deny(bare_trait_objects)]
#![warn(warnings)]
#![cfg_attr(feature = "codegen", feature(use_extern_macros))]
#![cfg_attr(feature = "extern-prelude", feature(extern_prelude))]
#![cfg_attr(feature = "nightly", feature(macro_vis_matcher))]

extern crate bytes;
extern crate cookie;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
extern crate indexmap;
#[macro_use]
extern crate log;
extern crate mime;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate tokio;
extern crate tokio_tcp;
extern crate tokio_threadpool;
#[cfg(unix)]
extern crate tokio_uds;
#[cfg(test)]
#[macro_use]
extern crate matches;

#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

#[cfg(feature = "codegen")]
extern crate tsukuyomi_codegen;

pub mod app;
pub mod error;
pub mod handler;
pub mod input;
pub mod json;
pub mod local;
pub mod modifier;
pub mod output;
pub mod server;

#[doc(inline)]
pub use app::App;

#[doc(inline)]
pub use error::{Error, Result};

#[doc(inline)]
pub use handler::Handler;

#[doc(inline)]
pub use input::Input;

#[doc(inline)]
pub use modifier::Modifier;

#[doc(inline)]
pub use output::{AsyncResponder, Output, Responder};

#[cfg(feature = "codegen")]
pub use tsukuyomi_codegen::{async_handler, handler};

/// A type alias of `Result<T, E>` which will be returned from `run`.
///
/// This typed is intended to be used as the return type of `main()`.
pub type AppResult<T> = ::std::result::Result<T, ::failure::Error>;

/// Starts an HTTP server with a constructed `App` and the default server configuration.
pub fn run(app: App) -> AppResult<()> {
    let server = ::server::Server::builder().finish(app)?;
    server.serve();
    Ok(())
}
