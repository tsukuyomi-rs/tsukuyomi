use {
    super::{
        concurrency::{imp::ConcurrencyImpl, Concurrency, DefaultConcurrency},
        recognizer::Captures,
        AppInner, Endpoint,
    },
    crate::{
        input::{
            body::RequestBody,
            localmap::{LocalData, LocalMap},
            param::Params,
            Cookies, Input,
        },
        output::{IntoResponse, ResponseBody},
        upgrade::Upgraded,
        util::Never,
    },
    cookie::CookieJar,
    futures01::{Async, Future, Poll},
    http::{
        header::{self, HeaderMap},
        Request, Response,
    },
    izanami::{
        http::{HttpBody, HttpUpgrade},
        service::Service,
    },
    std::{fmt, marker::PhantomData, net::SocketAddr, sync::Arc},
    tokio_buf::BufStream,
    tokio_io::{AsyncRead, AsyncWrite},
};

macro_rules! ready {
    ($e:expr) => {
        match $e {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(x)) => Ok(x),
            Err(e) => Err(e),
        }
    };
}

/// The instance of `Service` generated by `App`.
#[derive(Debug)]
pub struct AppService<C: Concurrency = DefaultConcurrency> {
    inner: Arc<AppInner<C>>,
    remote_addr: Option<SocketAddr>,
}

impl<C: Concurrency> AppService<C> {
    pub(super) fn new(inner: Arc<AppInner<C>>) -> Self {
        Self {
            inner,
            remote_addr: None,
        }
    }

    pub fn remote_addr(self, addr: SocketAddr) -> Self {
        Self {
            remote_addr: Some(addr),
            ..self
        }
    }
}

impl<C, Bd> Service<Request<Bd>> for AppService<C>
where
    C: Concurrency,
    RequestBody: From<Bd>,
{
    type Response = Response<AppBody<C>>;
    type Error = Never;
    type Future = AppFuture<C>;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    #[inline]
    fn call(&mut self, request: Request<Bd>) -> Self::Future {
        let (parts, body) = request.into_parts();

        let mut locals = LocalMap::default();
        RequestBody::from(body).insert_into(&mut locals);

        if let Some(addr) = self.remote_addr {
            locals.insert(&super::REMOTE_ADDR, addr);
        }

        AppFuture {
            request: Request::from_parts(parts, ()),
            inner: self.inner.clone(),
            cookie_jar: None,
            response_headers: None,
            locals,
            endpoint: None,
            captures: None,
            state: AppFutureState::Init,
        }
    }
}

/// A future that manages an HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppFuture<C: Concurrency> {
    request: Request<()>,
    inner: Arc<AppInner<C>>,
    cookie_jar: Option<CookieJar>,
    response_headers: Option<HeaderMap>,
    locals: LocalMap,
    endpoint: Option<Arc<Endpoint<C>>>,
    captures: Option<Captures>,
    state: AppFutureState<C>,
}

enum AppFutureState<C: Concurrency> {
    Init,
    InFlight(<C::Impl as ConcurrencyImpl>::Handle),
    Done,
}

impl<C: Concurrency> fmt::Debug for AppFutureState<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppFutureState::Init => f.debug_struct("Init").finish(),
            AppFutureState::InFlight(..) => f.debug_struct("InFlight").finish(),
            AppFutureState::Done => f.debug_struct("Done").finish(),
        }
    }
}

macro_rules! input {
    ($self:expr) => {
        &mut Input {
            request: &$self.request,
            params: {
                &if let Some(ref endpoint) = $self.endpoint {
                    Some(Params {
                        path: $self.request.uri().path(),
                        names: endpoint.uri.capture_names(),
                        captures: $self.captures.as_ref(),
                    })
                } else {
                    None
                }
            },
            cookies: &mut Cookies::new(&mut $self.cookie_jar, &$self.request),
            locals: &mut $self.locals,
            response_headers: &mut $self.response_headers,
            _marker: PhantomData,
        }
    };
}

impl<C: Concurrency> AppFuture<C> {
    fn process_recognize(&mut self) -> Result<<C::Impl as ConcurrencyImpl>::Handle, crate::Error> {
        self.endpoint = None;
        self.captures = None;

        match self
            .inner
            .find_endpoint(self.request.uri().path(), &mut self.captures)
        {
            Ok(endpoint) => {
                self.endpoint = Some(endpoint.clone());
                Ok(<C::Impl as ConcurrencyImpl>::handle(&endpoint.handler))
            }
            Err(scope) => match self.inner.find_default_handler(scope.id()) {
                Some(fallback) => Ok(<C::Impl as ConcurrencyImpl>::handle(fallback)),
                None => Err(http::StatusCode::NOT_FOUND.into()),
            },
        }
    }

    fn process_before_reply(&mut self, output: &mut Response<ResponseBody>) {
        // append Cookie entries.
        if let Some(ref jar) = self.cookie_jar {
            for cookie in jar.delta() {
                output.headers_mut().append(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }

        // append supplemental response headers.
        if let Some(mut hdrs) = self.response_headers.take() {
            for (k, v) in hdrs.drain() {
                output.headers_mut().extend(v.map(|v| (k.clone(), v)));
            }
        }
    }
}

impl<C: Concurrency> Future for AppFuture<C> {
    type Item = Response<AppBody<C>>;
    type Error = Never;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let polled = loop {
            self.state = match self.state {
                AppFutureState::Init => match self.process_recognize() {
                    Ok(in_flight) => AppFutureState::InFlight(in_flight),
                    Err(err) => break Err(err),
                },
                AppFutureState::InFlight(ref mut in_flight) => {
                    break ready!(<C::Impl as ConcurrencyImpl>::poll_ready_handle(
                        in_flight,
                        input!(self)
                    ));
                }
                AppFutureState::Done => panic!("the future has already polled."),
            };
        };
        self.state = AppFutureState::Done;

        let (mut output, upgrade) = match polled {
            Ok(output) => output,
            Err(err) => (err.into_response(), None),
        };

        self.process_before_reply(&mut output);

        Ok(Async::Ready(
            output.map(move |data| AppBody { data, upgrade }),
        ))
    }
}

#[allow(missing_debug_implementations)]
pub struct AppBody<C: Concurrency = DefaultConcurrency> {
    data: ResponseBody,
    upgrade: Option<<C::Impl as ConcurrencyImpl>::Upgrade>,
}

impl<C: Concurrency> AppBody<C> {
    pub(crate) fn into_response_body(self) -> ResponseBody {
        self.data
    }
}

impl<C: Concurrency> HttpBody for AppBody<C> {
    type Data = <ResponseBody as BufStream>::Item;
    type Error = <ResponseBody as BufStream>::Error;

    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        self.data.poll_buf()
    }
}

impl<C: Concurrency, I> HttpUpgrade<I> for AppBody<C>
where
    C: Concurrency,
    I: AsyncRead + AsyncWrite + 'static,
{
    type Upgraded = AppConnection<C, I>;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn upgrade(self, io: I) -> Result<Self::Upgraded, I> {
        match self.upgrade {
            Some(upgrade) => Ok(AppConnection { upgrade, io }),
            None => Err(io),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct AppConnection<C: Concurrency, I> {
    upgrade: <C::Impl as ConcurrencyImpl>::Upgrade,
    io: I,
}

impl<C, I> izanami::http::Connection for AppConnection<C, I>
where
    C: Concurrency,
    I: AsyncRead + AsyncWrite + 'static,
{
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        <C::Impl as ConcurrencyImpl>::poll_upgrade(
            &mut self.upgrade,
            &mut Upgraded::new(&mut self.io),
        )
    }

    fn graceful_shutdown(&mut self) {
        <C::Impl as ConcurrencyImpl>::close_upgrade(&mut self.upgrade)
    }
}
