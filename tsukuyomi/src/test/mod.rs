#![allow(missing_docs)]

mod error;
mod input;
mod output;
mod server;

pub use self::error::{Error, Result};
pub use self::input::{IntoRequestBody, TestInput};
pub use self::output::TestOutput;
pub use self::server::{Client, TestServer};

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
