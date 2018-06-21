//! [unstable]
//! A testing framework for Tsukuyomi.
//!
//! # Examples
//!
//! ```
//! # extern crate tsukuyomi;
//! # extern crate http;
//! # use tsukuyomi::app::App;
//! # use tsukuyomi::test::TestServer;
//! # use http::{StatusCode, header};
//! #
//! let app = App::builder()
//!     .mount("/", |m| {
//!         m.get("/hello").handle(|_| "Hello");
//!     })
//!     .finish()
//!     .unwrap();
//!
//! let mut server = TestServer::new(app).unwrap();
//!
//! let mut client = server.client();
//!
//! let response = client.get("/hello")
//!     .header("X-API-key", "dummy")
//!     .body(())
//!     .unwrap();
//! assert_eq!(response.status(), StatusCode::OK);
//! assert!(response.headers().contains_key(header::CONTENT_TYPE));
//! assert_eq!(*response.body().to_bytes(), b"Hello"[..]);
//! ```

// TODO: emulates some behaviour of Hyper

#![allow(missing_docs)]

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

#[derive(Debug)]
pub struct TestServer {
    app: App,
    runtime: Runtime,
}

impl TestServer {
    /// Creates a new instance of `TestServer` from a configured `App`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(app: App) -> io::Result<TestServer> {
        Ok(TestServer {
            app: app,
            runtime: Runtime::new()?,
        })
    }

    pub fn client<'a>(&'a mut self) -> Client<'a> {
        Client {
            service: self.app.new_service(),
            runtime: &mut self.runtime,
        }
    }
}

#[derive(Debug)]
pub struct Client<'a> {
    service: AppService,
    runtime: &'a mut Runtime,
}

macro_rules! impl_methods_for_client {
    ($(
        $name:ident => $METHOD:ident,
    )*) => {$(
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
            request: request,
        }
    }

    impl_methods_for_client![
        get => GET,
        post => POST,
        put => PUT,
        delete => DELETE,
        head => HEAD,
        patch => PATCH,
    ];
}

/// A type representing a dummy HTTP request from a peer.
///
/// The signature of methods in this type are intentionally same as `request::Builder`.
#[derive(Debug)]
pub struct LocalRequest<'a: 'b, 'b> {
    client: Option<&'b mut Client<'a>>,
    request: request::Builder,
}

impl<'a, 'b> LocalRequest<'a, 'b> {
    pub fn method<M>(&mut self, method: M) -> &mut LocalRequest<'a, 'b>
    where
        Method: HttpTryFrom<M>,
    {
        self.request.method(method);
        self
    }

    pub fn uri<U>(&mut self, uri: U) -> &mut LocalRequest<'a, 'b>
    where
        Uri: HttpTryFrom<U>,
    {
        self.request.uri(uri);
        self
    }

    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut LocalRequest<'a, 'b>
    where
        HeaderName: HttpTryFrom<K>,
        HeaderValue: HttpTryFrom<V>,
    {
        self.request.header(key, value);
        self
    }

    fn take(&mut self) -> LocalRequest<'a, 'b> {
        LocalRequest {
            client: self.client.take(),
            request: mem::replace(&mut self.request, Request::builder()),
        }
    }

    pub fn body<T>(&mut self, body: T) -> Result<Response<Data>, CritError>
    where
        T: Into<RequestBody>,
    {
        let LocalRequest { client, mut request } = self.take();

        let client = client.expect("This LocalRequest has already been used.");
        let request = request.body(body.into())?;

        let future = client.service.dispatch_request(request);
        client.runtime.block_on(TestResponseFuture::Initial(future))
    }
}

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
                TestResponseFuture::Initial(ref mut f) => Some(Polled::Response(try_ready!(f.poll_ready().into()))),
                TestResponseFuture::Receive(ref mut res) => {
                    Some(Polled::Received(try_ready!(res.body_mut().poll_ready().into())))
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
