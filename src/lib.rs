// #![warn(missing_docs)]
// #![warn(missing_debug_implementations)]
// #![warn(bare_trait_object)]
// #![warn(unreachable_pub)]
// #![warn(unused_extern_crates)]

//! Ganymede is a next generation Web framework for Rust.

extern crate bytes;
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
#[macro_use]
extern crate scoped_tls;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate tokio;
extern crate tokio_tcp;
#[cfg(unix)]
extern crate tokio_uds;

#[cfg(feature = "session")]
extern crate cookie;

#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

pub mod app;
pub mod context;
pub mod error;
pub mod input;
pub mod json;
pub mod output;
pub mod router;
pub mod server;
pub mod upgrade;

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

mod handler;
#[cfg(feature = "session")]
mod session;

#[doc(inline)]
pub use app::App;
#[doc(inline)]
pub use context::Context;
#[doc(inline)]
pub use error::{Error, Result};
#[doc(inline)]
pub use handler::Handler;
#[doc(inline)]
pub use output::{Output, Responder};
#[doc(inline)]
pub use router::Route;

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
