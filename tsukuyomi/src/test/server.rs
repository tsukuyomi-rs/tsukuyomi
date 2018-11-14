use std::io;
use std::mem;

use futures::{Future, Poll};
use http;
use http::Response;
use hyper::body::{Body, Payload};
use tokio::executor::thread_pool::Builder as ThreadPoolBuilder;
use tokio::runtime;
use tokio::runtime::Runtime;
use tower_service::{NewService, Service};

use crate::server::imp::CritError;
use crate::server::{HttpRequest, HttpResponse};

use super::input::TestInput;
use super::output::{Receive, TestOutput};

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub fn test_server<S>(new_service: S) -> TestServer<S>
where
    S: NewService + Send + 'static,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as Service>::Future: Send + 'static,
    S::InitError: Into<CritError> + Send + 'static,
{
    TestServer::new(new_service).expect("failed to initialize the runtime")
}

/// A local server which emulates an HTTP service without using
/// the low-level transport.
///
/// The value of this struct conttains an instance of `NewHttpService`
/// and a Tokio runtime.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct TestServer<S> {
    new_service: S,
    runtime: Runtime,
}

impl<S> TestServer<S>
where
    S: NewService + Send + 'static,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as Service>::Future: Send + 'static,
    S::InitError: Into<CritError> + Send + 'static,
{
    /// Creates a new instance of `LocalServer` from a `NewHttpService`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(new_service: S) -> io::Result<Self> {
        let mut pool = ThreadPoolBuilder::new();
        pool.pool_size(1);

        let runtime = runtime::Builder::new()
            .core_threads(1)
            .blocking_threads(1)
            .build()?;

        Ok(Self {
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

    pub fn perform<T>(&mut self, input: T) -> Result<Response<TestOutput>, CritError>
    where
        T: TestInput,
        <S::Service as Service>::Future: Send + 'static,
        S::InitError: Into<CritError>,
    {
        let mut client = self.client().map_err(Into::into)?;
        client.perform(input).map_err(Into::into)
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
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    S::Future: Send + 'static,
{
    /// Applies an HTTP request to this client and get its response.
    pub fn perform<T>(&mut self, input: T) -> Result<Response<TestOutput>, CritError>
    where
        T: TestInput,
    {
        let request = input.build_request()?;
        let request =
            S::Request::from_request(request.map(<S::Request as HttpRequest>::Body::from));
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
    type Item = Response<TestOutput>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::TestResponseFuture::*;
        loop {
            let response = match *self {
                Initial(ref mut f) => {
                    let response = futures::try_ready!(f.poll().map_err(Into::into));
                    Some(response)
                }
                Receive(_, ref mut receive) => {
                    futures::try_ready!(receive.poll_ready().map_err(Into::into));
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
