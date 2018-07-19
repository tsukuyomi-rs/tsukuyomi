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
//! use tsukuyomi::local::LocalServer;
//!
//! let app = App::builder()
//!     .route(("/hello", handler::ready_handler(|_| "Hello")))
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
//!                     .get("/hello")
//!                     .execute()
//!                     .unwrap();
//!
//! // Do some stuff...
//! assert_eq!(response.status(), StatusCode::OK);
//! assert!(response.headers().contains_key(header::CONTENT_TYPE));
//! assert_eq!(*response.body().to_bytes(), b"Hello"[..]);
//! ```

// TODO: emulates some behaviour of Hyper

use futures::{Future, Poll};
use http::header::{HeaderName, HeaderValue};
use http::{request, HttpTryFrom, Method, Request, Response, Uri};
use std::{io, mem};
use tokio::runtime::current_thread::Runtime;

use app::service::{AppService, AppServiceFuture};
use app::App;
use error::CritError;
use input::body::RequestBody;
use output::{Data, Receive, ResponseBody};
use rt::{self, RuntimeMode};

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
            body: None,
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
    body: Option<RequestBody>,
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
    pub fn body<T>(&mut self, body: T) -> &mut LocalRequest<'a, 'b>
    where
        T: Into<RequestBody>,
    {
        self.body = Some(body.into());
        self
    }

    fn take(&mut self) -> LocalRequest<'a, 'b> {
        LocalRequest {
            client: self.client.take(),
            request: mem::replace(&mut self.request, Request::builder()),
            body: self.body.take(),
        }
    }

    /// Creates an HTTP request from the current configuration and retrieve its response.
    pub fn execute(&mut self) -> Result<Response<Data>, CritError> {
        let LocalRequest {
            client,
            mut request,
            body,
        } = self.take();

        let body = body.unwrap_or_else(|| RequestBody::from(()));

        let client = client.expect("This LocalRequest has already been used.");
        let request = request.body(body)?;

        let future = client.service.dispatch_request(request);
        rt::runtime::with_set_mode(RuntimeMode::CurrentThread, || {
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
        loop {
            let polled = match *self {
                TestResponseFuture::Initial(ref mut f) => Some(Polled::Response(try_ready!(f.poll_ready()))),
                TestResponseFuture::Receive(ref mut res) => {
                    Some(Polled::Received(try_ready!(res.body_mut().poll_ready())))
                }
                _ => unreachable!("unexpected state"),
            };

            match (mem::replace(self, TestResponseFuture::Done), polled) {
                (TestResponseFuture::Initial(..), Some(Polled::Response(response))) => {
                    *self = TestResponseFuture::Receive(response.map(ResponseBody::receive));
                }
                (TestResponseFuture::Receive(response), Some(Polled::Received(received))) => {
                    return Ok(response.map(|_| received).into())
                }
                _ => unreachable!("unexpected state"),
            }
        }
    }
}
