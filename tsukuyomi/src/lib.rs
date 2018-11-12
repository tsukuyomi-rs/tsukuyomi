//! Tsukuyomi is an asynchronous Web framework for Rust.

#![doc(html_root_url = "https://docs.rs/tsukuyomi/0.4.0-dev")]
#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]

extern crate tsukuyomi_internal as internal;
extern crate tsukuyomi_macros;
extern crate tsukuyomi_server;

extern crate bytes;
extern crate cookie;
extern crate either;
extern crate failure;
extern crate futures;
extern crate http;
extern crate indexmap;
extern crate mime;
extern crate serde;
extern crate tower_service;
extern crate url;
extern crate uuid;

#[cfg(test)]
extern crate matches;

pub mod app;
pub mod error;
pub mod extractor;
pub mod output;

pub mod input {
    pub use crate::app::service::input::*;
}

#[doc(hidden)]
pub use tsukuyomi_macros::route_expr_impl;
pub use tsukuyomi_server::server::server;
pub use tsukuyomi_server::test;

#[allow(missing_docs)]
pub mod rt {
    pub use tsukuyomi_server::rt::*;

    use crate::error::Error;
    use futures::{Async, Future, Poll};

    pub fn blocking_section<F, T, E>(op: F) -> BlockingSection<F>
    where
        F: FnOnce() -> Result<T, E>,
        E: Into<Error>,
    {
        BlockingSection { op: Some(op) }
    }

    #[derive(Debug)]
    pub struct BlockingSection<F> {
        op: Option<F>,
    }

    #[cfg_attr(feature = "cargo-clippy", allow(redundant_closure))]
    impl<F, T, E> Future for BlockingSection<F>
    where
        F: FnOnce() -> Result<T, E>,
        E: Into<Error>,
    {
        type Item = T;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match blocking(|| (self.op.take().unwrap())()) {
                Ok(Async::Ready(Ok(x))) => Ok(Async::Ready(x)),
                Ok(Async::Ready(Err(e))) => Err(e.into()),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(err) => Err(crate::error::internal_server_error(err)),
            }
        }
    }
}

pub mod server {
    pub use tsukuyomi_server::server::*;
    pub use tsukuyomi_server::service;
}
