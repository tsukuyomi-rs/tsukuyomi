//! Ganymede is a next generation of Web framework for Rust.

extern crate bytes;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
extern crate log;
extern crate tokio;
#[macro_use]
extern crate scoped_tls;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

pub mod context;
pub mod error;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod service;

use router::Router;
use std::net::SocketAddr;

pub fn launch(router: Router, addr: &SocketAddr) -> Result<()> {
    let new_service = service::NewMyService::new(router);
    server::serve(new_service, addr)
}
