//! Components for integrating GraphQL endpoints into Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-juniper/0.2.0-dev")]
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
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(stutter))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate bytes;
extern crate futures;
extern crate http;
extern crate juniper;
extern crate percent_encoding;
extern crate tsukuyomi;

mod executor;
mod graphiql;
mod request;

pub use crate::{
    executor::{executor, Executor, Schema},
    graphiql::graphiql_source,
};
