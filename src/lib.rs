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
pub mod context;
pub mod error;
pub mod handler;
pub mod input;
pub mod output;
pub mod router;
pub mod rt;
pub mod transport;
pub mod upgrade;
