// FIXME: remove this feature gate as soon as the rustc version used in docs.rs is updated
#![cfg_attr(tsukuyomi_inject_extern_prelude, feature(extern_prelude))]

//! Tsukuyomi is a next generation Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.2.2")]
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

extern crate bytes;
extern crate cookie;
extern crate either;
#[macro_use]
extern crate failure;
extern crate filetime;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
#[cfg_attr(test, macro_use)]
extern crate indexmap;
#[macro_use]
extern crate log;
extern crate mime;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate time;
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

#[cfg(feature = "websocket")]
extern crate base64;
#[cfg(feature = "websocket")]
extern crate sha1;
#[cfg(feature = "websocket")]
extern crate tokio_tungstenite;
#[cfg(feature = "websocket")]
extern crate tungstenite;

pub mod app;
pub mod error;
pub mod fs;
pub mod handler;
pub mod input;
pub mod json;
pub mod modifier;
pub mod output;
pub(crate) mod recognizer;
pub mod server;

#[cfg(feature = "websocket")]
pub mod websocket;

#[doc(inline)]
pub use app::App;

#[doc(inline)]
pub use error::{Error, HttpError, Result};

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
