//! A testing framework for Tsukuyomi.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate http;
//! # use tsukuyomi::app::App;
//! # use tsukuyomi::handler;
//! # use http::{Request, StatusCode, header};
//! use tsukuyomi::server::local::LocalServer;
//!
//! let app = App::builder()
//!     .route(("/hello", handler::wrap_ready(|_| "Hello")))
//!     .finish()
//!     .unwrap();
//!
//! // Create a local server from an App.
//! // The instance emulates the behavior of an HTTP service
//! // without the low level I/O.
//! let mut server = LocalServer::new(app).unwrap();
//!
//! // Emulate an HTTP request and retrieve its response.
//! let request = Request::get("/hello")
//!     .body(Default::default())
//!     .expect("should be a valid HTTP request");
//! let response = server.client()
//!     .perform(request)
//!     .expect("unrecoverable error");
//!
//! // Do some stuff...
//! assert_eq!(response.status(), StatusCode::OK);
//! assert!(response.headers().contains_key(header::CONTENT_TYPE));
//! assert_eq!(*response.body().to_bytes(), b"Hello"[..]);
//! ```

// TODO: emulates some behaviour of Hyper

use std::borrow::Cow;
use std::io;
use std::mem;
use std::str;

use bytes::{Buf, Bytes};
use futures::{Async, Future, Poll};
use http::header::HeaderMap;
use http::{Request, Response};
use hyper::body::Payload;
use hyper::service::{NewService, Service};
use tokio::executor::thread_pool::Builder as ThreadPoolBuilder;
use tokio::runtime::{self, Runtime};

use super::CritError;

/// A local server which emulates an HTTP service without using the low-level transport.
///
/// This type wraps an `App` and a single-threaded Tokio runtime.
#[derive(Debug)]
pub struct LocalServer<S> {
    new_service: S,
    runtime: Runtime,
}

impl<S> LocalServer<S>
where
    S: NewService,
    S::Future: Send + 'static,
    S::Service: Send + 'static,
    <S::Service as Service>::ResBody: Payload,
    S::InitError: Send + 'static,
{
    /// Creates a new instance of `LocalServer` from a configured `App`.
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
    S: Service,
    S::ResBody: Payload,
    S::Future: Send + 'static,
{
    /// Applies an HTTP request to this client and get its response.
    pub fn perform(&mut self, request: Request<S::ReqBody>) -> Result<Response<Data>, CritError> {
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
    Receive(Response<Receive<Bd>>),
    Done,
}

enum Polled<Bd> {
    Response(Response<Bd>),
    Received(Data),
}

impl<F, Bd> Future for TestResponseFuture<F, Bd>
where
    F: Future<Item = Response<Bd>>,
    F::Error: Into<CritError>,
    Bd: Payload,
{
    type Item = Response<Data>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::TestResponseFuture::*;
        loop {
            let polled = match *self {
                Initial(ref mut f) => {
                    Some(Polled::Response(try_ready!(f.poll().map_err(Into::into))))
                }
                Receive(ref mut res) => Some(Polled::Received(try_ready!(
                    res.body_mut().poll().map_err(Into::into)
                ))),
                _ => unreachable!("unexpected state"),
            };

            match (mem::replace(self, TestResponseFuture::Done), polled) {
                (TestResponseFuture::Initial(..), Some(Polled::Response(response))) => {
                    *self = TestResponseFuture::Receive(response.map(self::Receive::new));
                }
                (TestResponseFuture::Receive(response), Some(Polled::Received(received))) => {
                    return Ok(response.map(|_| received).into())
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}

// ==== Data ====

#[derive(Debug)]
enum ReceiveState<Bd: Payload> {
    Init(Bd),
    ReceiveChunks(Bd, Vec<Bytes>),
    ReceiveTrailers(Bd, Vec<Bytes>),
    Gone,
}

#[derive(Debug)]
pub(crate) struct Receive<Bd: Payload> {
    state: ReceiveState<Bd>,
    content_length: Option<u64>,
}

impl<Bd: Payload> Receive<Bd> {
    fn new(body: Bd) -> Receive<Bd> {
        let content_length = body.content_length();
        Receive {
            state: ReceiveState::Init(body),
            content_length,
        }
    }
}

impl<Bd: Payload> Future for Receive<Bd> {
    type Item = Data;
    type Error = Bd::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let trailers = match self.state {
                ReceiveState::Init(..) => None,
                ReceiveState::ReceiveChunks(ref mut body, ref mut chunks) => {
                    while let Some(chunk) = try_ready!(body.poll_data()) {
                        chunks.push(chunk.collect());
                    }
                    None
                }
                ReceiveState::ReceiveTrailers(ref mut body, ..) => try_ready!(body.poll_trailers()),
                ReceiveState::Gone => panic!("The future has already polled"),
            };

            let old_state = mem::replace(&mut self.state, ReceiveState::Gone);
            match old_state {
                ReceiveState::Init(body) => {
                    self.state = ReceiveState::ReceiveChunks(body, vec![]);
                }
                ReceiveState::ReceiveChunks(body, chunks) => {
                    self.state = ReceiveState::ReceiveTrailers(body, chunks);
                }
                ReceiveState::ReceiveTrailers(_body, chunks) => {
                    return Ok(Async::Ready(Data {
                        chunks,
                        trailers,
                        content_length: self.content_length,
                    }))
                }
                ReceiveState::Gone => unreachable!("unexpected condition"),
            }
        }
    }
}

/// A type representing a received HTTP message data from the server.
///
/// This type is usually used by the testing framework.
#[derive(Debug)]
pub struct Data {
    chunks: Vec<Bytes>,
    trailers: Option<HeaderMap>,
    content_length: Option<u64>,
}

#[allow(missing_docs)]
impl Data {
    pub fn chunks(&self) -> &Vec<Bytes> {
        &self.chunks
    }

    pub fn trailers(&self) -> Option<&HeaderMap> {
        self.trailers.as_ref()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.content_length
    }

    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.chunks().iter().fold(Vec::new(), |mut acc, chunk| {
            acc.extend_from_slice(&*chunk);
            acc
        }))
    }

    pub fn to_utf8(&self) -> Result<Cow<'_, str>, str::Utf8Error> {
        match self.to_bytes() {
            Cow::Borrowed(bytes) => str::from_utf8(bytes).map(Cow::Borrowed),
            Cow::Owned(bytes) => String::from_utf8(bytes)
                .map_err(|e| e.utf8_error())
                .map(Cow::Owned),
        }
    }
}
