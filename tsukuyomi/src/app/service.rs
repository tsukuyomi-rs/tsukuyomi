use {
    super::{config::Concurrency, recognizer::Captures, AppInner, Endpoint},
    crate::{
        input::{
            body::RequestBody,
            localmap::{LocalData, LocalMap},
            param::Params,
            Cookies, Input,
        },
        output::ResponseBody,
        util::Never,
    },
    cookie::CookieJar,
    futures01::{Async, Future, Poll},
    http::{
        header::{self, HeaderMap, HeaderValue},
        Request, Response,
    },
    hyper::body::Payload,
    std::{fmt, marker::PhantomData, sync::Arc},
    tsukuyomi_service::Service,
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
pub struct AppService<C: Concurrency> {
    pub(super) inner: Arc<AppInner<C>>,
}

impl<C, Bd> Service<Request<Bd>> for AppService<C>
where
    C: Concurrency,
    RequestBody: From<Bd>,
{
    type Response = Response<ResponseBody>;
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
    InFlight(C::Handle),
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
    fn process_recognize(&mut self) -> Result<C::Handle, crate::Error> {
        self.endpoint = None;
        self.captures = None;

        match self
            .inner
            .find_endpoint(self.request.uri().path(), &mut self.captures)
        {
            Ok(endpoint) => {
                self.endpoint = Some(endpoint.clone());
                Ok(C::handle(&endpoint.handler))
            }
            Err(scope) => match self.inner.find_default_handler(scope.id()) {
                Some(fallback) => Ok(C::handle(fallback)),
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

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output
                .headers_mut()
                .entry(header::CONTENT_LENGTH)
                .expect("never fails")
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascii.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }
    }
}

impl<C: Concurrency> Future for AppFuture<C> {
    type Item = Response<ResponseBody>;
    type Error = Never;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let polled = loop {
            self.state = match self.state {
                AppFutureState::Init => match self.process_recognize() {
                    Ok(in_flight) => AppFutureState::InFlight(in_flight),
                    Err(err) => break Err(err),
                },
                AppFutureState::InFlight(ref mut in_flight) => {
                    break ready!(C::poll_ready(in_flight, input!(self)));
                }
                AppFutureState::Done => panic!("the future has already polled."),
            };
        };
        self.state = AppFutureState::Done;

        let mut output = match polled {
            Ok(output) => output,
            Err(err) => err.into_response(&self.request),
        };

        self.process_before_reply(&mut output);

        Ok(Async::Ready(output))
    }
}
