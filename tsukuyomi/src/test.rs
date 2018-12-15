#![allow(missing_docs)]

//! Utilities for testing HTTP services.

mod error;
mod input;
mod output;
mod server;

pub use self::{
    error::{Error, Result},
    input::{Input, IntoRequestBody},
    output::Output,
    server::{Server, Session},
};

use crate::service::MakeHttpService;

pub trait ResponseExt {
    fn header<H>(&self, name: H) -> Result<&http::header::HeaderValue>
    where
        H: http::header::AsHeaderName + std::fmt::Display;
}

impl<T> ResponseExt for http::Response<T> {
    fn header<H>(&self, name: H) -> Result<&http::header::HeaderValue>
    where
        H: http::header::AsHeaderName + std::fmt::Display,
    {
        let err = failure::format_err!("missing header field: `{}'", name);
        self.headers().get(name).ok_or_else(|| Error::from(err))
    }
}

pub fn server<S>(make_service: S) -> self::Result<Server<S, tokio::runtime::Runtime>>
where
    S: MakeHttpService<(), hyper::Body>,
{
    let mut builder = tokio::runtime::Builder::new();
    builder.core_threads(1);
    builder.blocking_threads(1);
    builder.name_prefix("tsukuyomi-test");
    let runtime = builder.build()?;
    Ok(Server::new(make_service, runtime))
}

pub fn current_thread_server<S>(
    make_service: S,
) -> self::Result<Server<S, tokio::runtime::current_thread::Runtime>>
where
    S: MakeHttpService<(), hyper::Body>,
{
    let runtime = tokio::runtime::current_thread::Runtime::new()?;
    Ok(Server::new(make_service, runtime))
}
