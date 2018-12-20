//! Utilities for testing HTTP services.

mod input;
mod output;
mod server;

pub use self::{
    input::{Input, IntoRequestBody},
    output::Output,
    server::{Server, Session},
};

use {
    http::{Request, Response},
    hyper::body::Payload,
    tsukuyomi_service::{MakeService, Service},
};

/// A set of extension methods of [`Response`] used within test cases.
///
/// [`Response`]: https://docs.rs/http/0.1/http/struct.Response.html
pub trait ResponseExt {
    /// Gets a reference to the header field with the specified name.
    ///
    /// If the header field does not exist, this method will return an `Err` instead of `None`.
    fn header<H>(&self, name: H) -> crate::Result<&http::header::HeaderValue>
    where
        H: http::header::AsHeaderName + std::fmt::Display;
}

impl<T> ResponseExt for http::Response<T> {
    fn header<H>(&self, name: H) -> crate::Result<&http::header::HeaderValue>
    where
        H: http::header::AsHeaderName + std::fmt::Display,
    {
        let err = failure::format_err!("missing header field: `{}'", name);
        self.headers()
            .get(name)
            .ok_or_else(|| crate::Error::from(err))
    }
}

/// Creates a test server using the specified service factory.
pub fn server<S, Bd>(make_service: S) -> crate::Result<Server<S, tokio::runtime::Runtime>>
where
    S: MakeService<(), Request<hyper::Body>, Response = Response<Bd>>,
    S::Error: Into<crate::CritError>,
    S::Service: Send + 'static,
    <S::Service as Service<Request<hyper::Body>>>::Future: Send + 'static,
    S::MakeError: Into<crate::CritError>,
    S::Future: Send + 'static,
    Bd: Payload,
{
    let mut builder = tokio::runtime::Builder::new();
    builder.core_threads(1);
    builder.blocking_threads(1);
    builder.name_prefix("tsukuyomi-server");
    let runtime = builder.build()?;
    Ok(Server::new(make_service, runtime))
}

/// Creates a test server that exexutes all task onto a single thread,
/// using the specified service factory.
pub fn local_server<S, Bd>(
    make_service: S,
) -> crate::Result<Server<S, tokio::runtime::current_thread::Runtime>>
where
    S: MakeService<(), Request<hyper::Body>, Response = Response<Bd>>,
    S::Error: Into<crate::CritError>,
    S::MakeError: Into<crate::CritError>,
    Bd: Payload,
{
    let runtime = tokio::runtime::current_thread::Runtime::new()?;
    Ok(Server::new(make_service, runtime))
}
