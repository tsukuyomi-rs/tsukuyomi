extern crate bytes;
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
pub mod handler;
pub mod request;
pub mod response;
pub mod server;
pub mod service;

use handler::Handler;
use std::net::SocketAddr;

pub fn launch<H>(handler: H, addr: &SocketAddr) -> Result<()>
where
    H: Handler + Send + Sync + 'static,
    H::Future: Send,
{
    let new_service = service::new_service(handler);
    server::serve(new_service, addr)
}
