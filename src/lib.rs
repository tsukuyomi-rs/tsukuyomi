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
#[macro_use]
extern crate scoped_tls;

pub mod app;
pub mod error;
pub mod handler;
pub mod input;
pub mod output;
pub mod router;
pub mod transport;
pub mod upgrade;

mod context;
mod rt;

pub use app::App;
pub use context::Context;
pub use error::Error;
pub use output::{Output, Responder};

pub type Result<T> = ::std::result::Result<T, error::Error>;
