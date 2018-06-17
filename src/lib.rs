//! Tsukuyomi is a next generation Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.1.4")]
#![cfg_attr(feature = "nightly", feature(futures_api))]
#![cfg_attr(feature = "nightly", feature(extern_prelude))]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(unreachable_pub)]
#![deny(unused_extern_crates)]
#![deny(warnings)]

// #![deny(bare_trait_objects)]

extern crate bytes;
extern crate cookie;
#[macro_use]
extern crate failure;
extern crate fnv;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
extern crate hyperx;
#[macro_use]
extern crate log;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate state;
extern crate tokio;
extern crate tokio_tcp;
#[cfg(unix)]
extern crate tokio_uds;

#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

#[macro_use]
pub mod future;

pub mod app;
pub mod error;
pub mod input;
pub mod json;
pub mod modifier;
pub mod output;
pub mod router;
pub mod server;
pub mod upgrade;

#[cfg(feature = "session")]
pub mod session;

/// The definition of statically typed headers.
///
/// The items in this module are simply re-exports from `hyperx` crate.
/// See [the crate level documentation of hyperx][hyperx] for details.
///
/// [hyperx]: https://docs.rs/hyperx/0.12.*/
pub mod header {
    pub use hyperx::header::Header;
    pub use hyperx::header::*;
}

/// The definition of media types.
///
/// The items in this module are simply re-exports from `mime` crate.
/// See [the crate level documentation of mime][mime] for details.
///
/// [mime]: https://docs.rs/mime/0.3.*/
pub mod mime {
    extern crate mime;
    pub use self::mime::*;
    pub use self::mime::{Mime, Name, Params};
}

#[doc(inline)]
pub use app::App;

#[doc(inline)]
pub use error::{Error, Result};

#[doc(inline)]
pub use input::Input;

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
