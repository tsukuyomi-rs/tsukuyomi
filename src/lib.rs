//! Ganymede is a next generation of Web framework for Rust.

extern crate bytes;
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
#[cfg(feature = "tls")]
extern crate rustls;
#[cfg(feature = "tls")]
extern crate tokio_rustls;

pub mod app;
pub mod context;
pub mod error;
pub mod input;
pub mod output;
pub mod router;
pub mod transport;
pub mod upgrade;

mod handler;
mod rt;

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
