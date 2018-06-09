//! Ganymede is a next generation of Web framework for Rust.

extern crate bytes;
extern crate mime;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate tokio;
extern crate tokio_tcp;
#[cfg(unix)]
extern crate tokio_uds;
#[macro_use]
extern crate scoped_tls;
#[cfg(feature = "session")]
extern crate cookie;
extern crate fnv;
extern crate hyperx;
#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

extern crate serde;
extern crate serde_json;

pub mod app;
pub mod context;
pub mod error;
pub mod input;
pub mod json;
pub mod output;
pub mod server;
pub mod upgrade;

mod handler;
mod router;
#[cfg(feature = "session")]
mod session;

#[doc(inline)]
pub use app::App;
#[doc(inline)]
pub use context::Context;
#[doc(inline)]
pub use error::Error;
#[doc(inline)]
pub use handler::Handler;
#[doc(inline)]
pub use output::{Output, Responder};
#[doc(inline)]
pub use router::Route;

pub type Result<T> = ::std::result::Result<T, error::Error>;

pub type AppResult<T> = ::std::result::Result<T, ::failure::Error>;

pub fn run(app: App) -> AppResult<()> {
    let server = ::server::Server::builder().finish(app)?;
    server.serve();
    Ok(())
}
