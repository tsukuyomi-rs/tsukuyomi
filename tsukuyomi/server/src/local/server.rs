use std::io;
use std::mem;

use futures::{Future, Poll};
use http;
use http::Response;
use hyper::body::Payload;
use tokio::executor::thread_pool::Builder as ThreadPoolBuilder;
use tokio::runtime;
use tokio::runtime::Runtime;
use tower_service::{NewService, Service};

use crate::server::CritError;
use service::http::imp::{HttpRequestImpl, HttpResponseImpl};
use service::http::{HttpRequest, HttpResponse};

use super::data::{Data, Receive};
use super::input::Input;

/// A local server which emulates an HTTP service without using
/// the low-level transport.
///
/// The value of this struct conttains an instance of `NewHttpService`
/// and a Tokio runtime.
#[derive(Debug)]
pub struct LocalServer<S> {
    new_service: S,
    runtime: Runtime,
}

impl<S> LocalServer<S>
where
    S: NewService + Send + 'static,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    S::InitError: Send + 'static,
{
    /// Creates a new instance of `LocalServer` from a `NewHttpService`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(new_service: S) -> io::Result<LocalServer<S>> {
        let mut pool = ThreadPoolBuilder::new();
        pool.pool_size(1);

        let runtime = runtime::Builder::new()
            .core_threads(1)
            .blocking_threads(1)
            .build()?;

        Ok(LocalServer {
            new_service,
            runtime,
        })
    }

    /// Create a `Client` associated with this server.
    pub fn client(&mut self) -> Result<Client<'_, S::Service>, S::InitError> {
        let service = self.runtime.block_on(self.new_service.new_service())?;
        Ok(Client {
            service,
            runtime: &mut self.runtime,
        })
    }
}

/// A type which emulates a connection to a peer.
#[derive(Debug)]
pub struct Client<'a, S> {
    service: S,
    runtime: &'a mut Runtime,
}

impl<'a, S> Client<'a, S>
where
    S: Service + Send + 'static,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
{
    /// Applies an HTTP request to this client and get its response.
    pub fn perform<T>(&mut self, input: T) -> Result<Response<Data>, CritError>
    where
        T: Input,
    {
        let input = input.build_request()?;
        let request = S::Request::from_request(input);
        let future = TestResponseFuture::Initial(self.service.call(request));
        self.runtime.block_on(future)
    }

    /// Returns the reference to the underlying Tokio runtime.
    pub fn runtime(&mut self) -> &mut Runtime {
        &mut *self.runtime
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum TestResponseFuture<F, Bd: Payload> {
    Initial(F),
    Receive(http::response::Parts, Receive<Bd>),
    Done,
}

impl<F, Bd> Future for TestResponseFuture<F, Bd>
where
    F: Future,
    F::Item: HttpResponse<Body = Bd>,
    F::Error: Into<CritError>,
    Bd: Payload,
{
    type Item = Response<Data>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::TestResponseFuture::*;
        loop {
            let response = match *self {
                Initial(ref mut f) => {
                    let response = try_ready!(f.poll().map_err(Into::into));
                    Some(response)
                }
                Receive(_, ref mut receive) => {
                    try_ready!(receive.poll_ready().map_err(Into::into));
                    None
                }
                _ => unreachable!("unexpected state"),
            };

            match mem::replace(self, TestResponseFuture::Done) {
                TestResponseFuture::Initial(..) => {
                    let response = response.expect("unexpected condition");
                    let (parts, body) = response.into_response().into_parts();
                    let receive = self::Receive::new(body);
                    *self = TestResponseFuture::Receive(parts, receive);
                }
                TestResponseFuture::Receive(parts, receive) => {
                    let data = receive.into_data().expect("unexpected condition");
                    let response = Response::from_parts(parts, data);
                    return Ok(response.into());
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}
