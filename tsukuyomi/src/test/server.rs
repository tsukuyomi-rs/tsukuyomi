use {
    super::{
        input::TestInput,
        output::{Receive, TestOutput},
    },
    crate::server::{
        imp::CritError,
        middleware::{Identity, Middleware},
        HttpRequest, HttpResponse,
    },
    futures::{Future, Poll},
    http::Response,
    hyper::body::{Body, Payload},
    std::{
        mem,
        panic::{resume_unwind, AssertUnwindSafe},
        sync::Arc,
    },
    tokio::{
        executor::thread_pool::Builder as ThreadPoolBuilder,
        runtime::{self, Runtime},
    },
    tower_service::{NewService, Service},
};

/// A local server which emulates an HTTP service without using
/// the low-level transport.
///
/// The value of this struct conttains an instance of `NewHttpService`
/// and a Tokio runtime.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct TestServer<S, M = Identity> {
    new_service: S,
    middleware: Arc<M>,
    runtime: Runtime,
}

impl<S> TestServer<S>
where
    S: NewService,
{
    /// Creates a new instance of `LocalServer` from a `NewHttpService`.
    ///
    /// This function will return an error if the construction of the runtime is failed.
    pub fn new(new_service: S) -> super::Result<Self> {
        let mut pool = ThreadPoolBuilder::new();
        pool.pool_size(1);

        let runtime = runtime::Builder::new()
            .core_threads(1)
            .blocking_threads(1)
            .build()?;

        Ok(Self {
            new_service,
            middleware: Arc::new(Identity::default()),
            runtime,
        })
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, M> TestServer<S, M>
where
    S: NewService,
    M: Middleware<S::Service>,
{
    pub fn with_middleware<N>(self, middleware: N) -> TestServer<S, N>
    where
        N: Middleware<S::Service>,
    {
        TestServer {
            new_service: self.new_service,
            middleware: Arc::new(middleware),
            runtime: self.runtime,
        }
    }
}

impl<S, M> TestServer<S, M>
where
    S: NewService + Send + 'static,
    S::Future: Send + 'static,
    S::InitError: Into<CritError> + Send + 'static,
    M: Middleware<S::Service> + Send + Sync + 'static,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    M::Service: Send + 'static,
    <M::Service as Service>::Future: Send + 'static,
{
    /// Create a `Client` associated with this server.
    pub fn client(&mut self) -> super::Result<Client<'_, M::Service>> {
        let middleware = self.middleware.clone();
        let service = match self.runtime.block_on(
            AssertUnwindSafe(
                self.new_service
                    .new_service()
                    .map(move |service| middleware.wrap(service)),
            ).catch_unwind(),
        ) {
            Ok(result) => result.map_err(|err| failure::Error::from_boxed_compat(err.into()))?,
            Err(err) => resume_unwind(Box::new(err)),
        };
        Ok(Client {
            service,
            runtime: &mut self.runtime,
        })
    }

    pub fn perform<T>(&mut self, input: T) -> super::Result<Response<TestOutput>>
    where
        T: TestInput,
    {
        let mut client = self.client()?;
        client.perform(input)
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
    pub fn perform<T>(&mut self, input: T) -> super::Result<Response<TestOutput>>
    where
        T: TestInput,
    {
        let request = input.build_request()?;
        let request =
            S::Request::from_request(request.map(<S::Request as HttpRequest>::Body::from));
        let future = TestResponseFuture::Initial(self.service.call(request));
        match self
            .runtime
            .block_on(AssertUnwindSafe(future).catch_unwind())
        {
            Ok(result) => result.map_err(|err| failure::Error::from_boxed_compat(err).into()),
            Err(err) => resume_unwind(Box::new(err)),
        }
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
