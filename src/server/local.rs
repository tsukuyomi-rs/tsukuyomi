//! A testing framework for Tsukuyomi.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate http;
//! # use tsukuyomi::app::App;
//! # use tsukuyomi::handler;
//! # use http::{StatusCode, header};
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
//! let response = server.client()
//!     .get("/hello")
//!     .execute()
//!     .unwrap();
//!
//! // Do some stuff...
//! assert_eq!(response.status(), StatusCode::OK);
//! assert!(response.headers().contains_key(header::CONTENT_TYPE));
//! assert_eq!(*response.body().to_bytes(), b"Hello"[..]);
//! ```

// TODO: emulates some behaviour of Hyper

use bytes::Bytes;
use futures::{Async, Future, Poll, Stream};
use http::header::{HeaderName, HeaderValue};
use http::{request, HttpTryFrom, Method, Request, Response, Uri};
use hyper::Body;
use std::borrow::Cow;
use std::{io, mem, str};
use tokio::executor::DefaultExecutor;
use tokio::runtime::current_thread::Runtime;

use app::service::{AppService, AppServiceFuture};
use app::App;
use error::CritError;
use input;
use output::{ResponseBody, ResponseBodyKind};

use super::rt::{with_set_mode, RuntimeMode};

/// A local server which emulates an HTTP service without using the low-level transport.
///
/// This type wraps an `App` and a single-threaded Tokio runtime.
#[derive(Debug)]
pub struct LocalServer {
    app: App,
    runtime: Runtime,
}

impl LocalServer {
    /// Creates a new instance of `LocalServer` from a configured `App`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(app: App) -> io::Result<LocalServer> {
        Ok(LocalServer {
            app,
            runtime: Runtime::new()?,
        })
    }

    /// Create a `Client` associated with this server.
    pub fn client(&mut self) -> Client {
        Client {
            service: self.app.new_service(),
            runtime: &mut self.runtime,
        }
    }
}

/// A type which emulates a connection to a peer.
#[derive(Debug)]
pub struct Client<'a> {
    service: AppService,
    runtime: &'a mut Runtime,
}

macro_rules! impl_methods_for_client {
    ($(
        $(#[$doc:meta])*
        $name:ident => $METHOD:ident,
    )*) => {$(
        $(#[$doc])*
        #[inline]
        pub fn $name<'b, U>(&'b mut self, uri: U) -> LocalRequest<'a, 'b>
        where
            Uri: HttpTryFrom<U>,
        {
            self.request(Method::$METHOD, uri)
        }
    )*};
}

impl<'a> Client<'a> {
    /// Create a `LocalRequest` associated with this client.
    pub fn request<'b, M, U>(&'b mut self, method: M, uri: U) -> LocalRequest<'a, 'b>
    where
        Method: HttpTryFrom<M>,
        Uri: HttpTryFrom<U>,
    {
        let mut request = Request::builder();
        request.method(method);
        request.uri(uri);

        LocalRequest {
            client: Some(self),
            request,
            body: Default::default(),
        }
    }

    impl_methods_for_client![
        /// Equivalent to `Client::request(Method::GET, uri)`.
        get => GET,
        /// Equivalent to `Client::request(Method::POST, uri)`.
        post => POST,
        /// Equivalent to `Client::request(Method::PUT, uri)`.
        put => PUT,
        /// Equivalent to `Client::request(Method::DELETE, uri)`.
        delete => DELETE,
        /// Equivalent to `Client::request(Method::HEAD, uri)`.
        head => HEAD,
        /// Equivalent to `Client::request(Method::PATCH, uri)`.
        patch => PATCH,
    ];
}

/// A type which emulates an HTTP request from a peer.
#[derive(Debug)]
pub struct LocalRequest<'a: 'b, 'b> {
    client: Option<&'b mut Client<'a>>,
    request: request::Builder,
    body: RequestBody,
}

impl<'a, 'b> LocalRequest<'a, 'b> {
    /// Modifies the value of HTTP method of this request.
    pub fn method<M>(&mut self, method: M) -> &mut LocalRequest<'a, 'b>
    where
        Method: HttpTryFrom<M>,
    {
        self.request.method(method);
        self
    }

    /// Modifies the value of URI of this request.
    pub fn uri<U>(&mut self, uri: U) -> &mut LocalRequest<'a, 'b>
    where
        Uri: HttpTryFrom<U>,
    {
        self.request.uri(uri);
        self
    }

    /// Inserts a header value into this request.
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut LocalRequest<'a, 'b>
    where
        HeaderName: HttpTryFrom<K>,
        HeaderValue: HttpTryFrom<V>,
    {
        self.request.header(key, value);
        self
    }

    /// Sets a message body of this request.
    pub fn body(&mut self, body: impl Into<RequestBody>) -> &mut LocalRequest<'a, 'b> {
        self.body = body.into();
        self
    }

    fn take(&mut self) -> LocalRequest<'a, 'b> {
        LocalRequest {
            client: self.client.take(),
            request: mem::replace(&mut self.request, Request::builder()),
            body: mem::replace(&mut self.body, Default::default()),
        }
    }

    /// Creates an HTTP request from the current configuration and retrieve its response.
    pub fn execute(&mut self) -> Result<Response<Data>, CritError> {
        let LocalRequest {
            client,
            mut request,
            body: RequestBody(body),
        } = self.take();

        let client = client.expect("This LocalRequest has already been used.");
        let request = request.body(body)?;

        let future = client.service.dispatch_request(request);
        with_set_mode(RuntimeMode::CurrentThread, || {
            client.runtime.block_on(TestResponseFuture::Initial(future))
        })
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum TestResponseFuture {
    Initial(AppServiceFuture),
    Receive(Response<Receive>),
    Done,
}

enum Polled {
    Response(Response<ResponseBody>),
    Received(Data),
}

impl Future for TestResponseFuture {
    type Item = Response<Data>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // FIXME: use `futures::task::Context::executor()` instead.
        let mut exec = DefaultExecutor::current();
        loop {
            let polled = match *self {
                TestResponseFuture::Initial(ref mut f) => {
                    Some(Polled::Response(try_ready!(f.poll_ready(&mut exec))))
                }
                TestResponseFuture::Receive(ref mut res) => {
                    Some(Polled::Received(try_ready!(res.body_mut().poll_ready())))
                }
                _ => unreachable!("unexpected state"),
            };

            match (mem::replace(self, TestResponseFuture::Done), polled) {
                (TestResponseFuture::Initial(..), Some(Polled::Response(response))) => {
                    *self = TestResponseFuture::Receive(response.map(Receive::new));
                }
                (TestResponseFuture::Receive(response), Some(Polled::Received(received))) => {
                    return Ok(response.map(|_| received).into())
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct RequestBody(input::body::RequestBody);

impl RequestBody {
    fn from_hyp(body: Body) -> RequestBody {
        RequestBody(input::body::RequestBody::from_hyp(body))
    }
}

impl Default for RequestBody {
    fn default() -> Self {
        RequestBody::from_hyp(Default::default())
    }
}

impl From<()> for RequestBody {
    fn from(_: ()) -> Self {
        Default::default()
    }
}

macro_rules! impl_from_for_request_body {
    ($($t:ty,)*) => {$(
        impl From<$t> for RequestBody {
            fn from(body: $t) -> Self {
                RequestBody::from_hyp(body.into())
            }
        }
    )*};
}

impl_from_for_request_body![
    &'static str,
    &'static [u8],
    Vec<u8>,
    String,
    Cow<'static, str>,
    Cow<'static, [u8]>,
    Bytes,
];

// ==== Data ====

#[derive(Debug)]
pub(crate) struct Receive(ReceiveInner);

#[derive(Debug)]
enum ReceiveInner {
    Empty,
    Sized(Option<Bytes>),
    Chunked(Body, Vec<Bytes>),
}

impl Receive {
    fn new(body: ResponseBody) -> Receive {
        match body.0 {
            ResponseBodyKind::Empty => Receive(ReceiveInner::Empty),
            ResponseBodyKind::Sized(data) => Receive(ReceiveInner::Sized(Some(data))),
            ResponseBodyKind::Chunked(body) => Receive(ReceiveInner::Chunked(body, vec![])),
        }
    }

    pub(crate) fn poll_ready(&mut self) -> Poll<Data, CritError> {
        match self.0 {
            ReceiveInner::Empty => Ok(Async::Ready(Data(DataInner::Empty))),
            ReceiveInner::Sized(ref mut data) => Ok(Async::Ready(Data(DataInner::Sized(
                data.take().expect("The response body has already resolved"),
            )))),
            ReceiveInner::Chunked(ref mut body, ref mut chunks) => {
                while let Some(chunk) = try_ready!(body.poll()) {
                    chunks.push(chunk.into());
                }
                let chunks = mem::replace(chunks, vec![]);
                Ok(Async::Ready(Data(DataInner::Chunked(chunks))))
            }
        }
    }
}

/// A type representing a received HTTP message data from the server.
///
/// This type is usually used by the testing framework.
#[derive(Debug)]
pub struct Data(DataInner);

#[derive(Debug)]
enum DataInner {
    Empty,
    Sized(Bytes),
    Chunked(Vec<Bytes>),
}

#[allow(missing_docs)]
impl Data {
    pub fn is_sized(&self) -> bool {
        match self.0 {
            DataInner::Empty | DataInner::Sized(..) => true,
            _ => false,
        }
    }

    pub fn is_chunked(&self) -> bool {
        !self.is_sized()
    }

    pub fn content_length(&self) -> Option<usize> {
        match self.0 {
            DataInner::Empty => Some(0),
            DataInner::Sized(ref data) => Some(data.len()),
            _ => None,
        }
    }

    pub fn as_chunks(&self) -> Option<&[Bytes]> {
        match self.0 {
            DataInner::Chunked(ref chunks) => Some(&chunks[..]),
            _ => None,
        }
    }

    pub fn to_bytes(&self) -> Cow<[u8]> {
        match self.0 {
            DataInner::Empty => Cow::Borrowed(&[]),
            DataInner::Sized(ref data) => Cow::Borrowed(&data[..]),
            DataInner::Chunked(ref chunks) => {
                Cow::Owned(chunks.iter().fold(Vec::new(), |mut acc, chunk| {
                    acc.extend_from_slice(&*chunk);
                    acc
                }))
            }
        }
    }

    pub fn to_utf8(&self) -> Result<Cow<str>, str::Utf8Error> {
        match self.to_bytes() {
            Cow::Borrowed(bytes) => str::from_utf8(bytes).map(Cow::Borrowed),
            Cow::Owned(bytes) => String::from_utf8(bytes)
                .map_err(|e| e.utf8_error())
                .map(Cow::Owned),
        }
    }
}
