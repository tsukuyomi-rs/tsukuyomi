use {
    super::{callback::Context as CallbackContext, App, RouteData, RouteId},
    cookie::CookieJar,
    crate::{
        async_result::AsyncResult,
        error::{Critical, Error, HttpError},
        input::{local_map::LocalMap, Input, RequestBody},
        output::{Output, ResponseBody},
        recognizer::Captures,
    },
    futures::{Async, Future, IntoFuture, Poll},
    http::{
        header,
        header::{HeaderMap, HeaderValue},
        Method, Request, Response, StatusCode,
    },
    hyper::body::Payload,
    std::mem,
    tower_service::{NewService, Service},
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

pub fn app() -> super::builder::Builder<(), ()> {
    super::builder::Builder::default()
}

/// An instance of `HttpError` which will be thrown from the route recognizer.
///
/// The value of this type cannot be modified by the `Modifier`s since they will be
/// thrown before the scope will be determined.
#[derive(Debug, failure::Fail)]
pub enum RecognizeError {
    /// The request path is not matched to any routes.
    #[fail(display = "Not Found")]
    NotFound,

    /// The request path is matched but the method is invalid.
    #[fail(display = "Method Not Allowed")]
    MethodNotAllowed,
}

impl HttpError for RecognizeError {
    fn status_code(&self) -> StatusCode {
        match self {
            RecognizeError::NotFound => StatusCode::NOT_FOUND,
            RecognizeError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        }
    }
}

#[derive(Debug)]
pub enum Recognize<'a> {
    Matched(usize, Option<Captures>),
    FallbackHead(usize, Option<Captures>),
    FallbackOptions(&'a HeaderValue),
}

impl App {
    #[doc(hidden)]
    pub fn recognize(&self, path: &str, method: &Method) -> Result<Recognize<'_>, RecognizeError> {
        let (i, params) = self
            .data
            .recognizer
            .recognize(path)
            .ok_or_else(|| RecognizeError::NotFound)?;
        let endpoint = &self.data.endpoints[i];

        match endpoint.route_ids.get(method) {
            Some(&i) => Ok(Recognize::Matched(i, params)),
            None if self.data.config.fallback_head && *method == Method::HEAD => {
                match endpoint.route_ids.get(&Method::GET) {
                    Some(&i) => Ok(Recognize::FallbackHead(i, params)),
                    None => Err(RecognizeError::MethodNotAllowed),
                }
            }
            None if self.data.config.fallback_options && *method == Method::OPTIONS => {
                Ok(Recognize::FallbackOptions(&endpoint.allowed_methods))
            }
            None => Err(RecognizeError::MethodNotAllowed),
        }
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Service = Self;
    type InitError = Critical;
    type Future = futures::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures::future::ok(self.clone())
    }
}

impl Service for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Future = AppFuture;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    #[inline]
    fn call(&mut self, request: Self::Request) -> Self::Future {
        let (parts, body) = request.into_parts();
        AppFuture {
            state: AppFutureState::Init(body),
            app: self.clone(),
            request: Request::from_parts(parts, ()),
            locals: LocalMap::default(),
            response_headers: None,
            cookies: None,
        }
    }
}

/// A future for managing an incoming HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppFuture {
    state: AppFutureState,
    app: App,
    request: Request<()>,
    locals: LocalMap,
    response_headers: Option<HeaderMap>,
    cookies: Option<CookieJar>,
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum AppFutureState {
    Init(RequestBody),
    BeforeHandle {
        context: AppContext,
        in_flight: AsyncResult<Option<Output>>,
        pos: usize,
    },
    Handle {
        context: AppContext,
        in_flight: AsyncResult<Output>,
        pos: usize,
    },
    AfterHandle {
        context: AppContext,
        in_flight: AsyncResult<Output>,
        pos: usize,
    },
    Done,
}

macro_rules! callback_context {
    ($self:expr) => {
        &mut CallbackContext {
            request: &$self.request,
            locals: &mut $self.locals,
            response_headers: &mut $self.response_headers,
            cookies: &mut $self.cookies,
        }
    };
}

macro_rules! input {
    ($self:expr, $context:expr) => {
        &mut Input {
            app: &$self.app,
            request: &$self.request,
            locals: &mut $self.locals,
            response_headers: &mut $self.response_headers,
            cookies: &mut $self.cookies,
            context: $context,
        }
    };
}

impl AppFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppFutureState::*;

        #[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
        enum Polled {
            BeforeHandle(Option<Result<Output, Error>>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
            Empty,
        }

        let result = loop {
            let polled = match self.state {
                Init(..) => Polled::Empty,
                BeforeHandle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => {
                    // FIXME: use result.transpose()
                    Polled::BeforeHandle(
                        match ready!(in_flight.poll_ready(input!(self, context))) {
                            Ok(Some(x)) => Some(Ok(x)),
                            Ok(None) => None,
                            Err(e) => Some(Err(e)),
                        },
                    )
                }
                Handle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => Polled::Handle(ready!(in_flight.poll_ready(input!(self, context)))),
                AfterHandle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => Polled::AfterHandle(ready!(in_flight.poll_ready(input!(self, context)))),
                Done => panic!("unexpected state"),
            };

            self.state = match (mem::replace(&mut self.state, Done), polled) {
                (Init(body), Polled::Empty) => {
                    if let Some(output) = self.app.data.callback.on_init(callback_context!(self))? {
                        return Ok(Async::Ready(output));
                    }

                    let (pos, params) = match self
                        .app
                        .recognize(self.request.uri().path(), self.request.method())
                    {
                        Ok(Recognize::Matched(pos, captures))
                        | Ok(Recognize::FallbackHead(pos, captures)) => (pos, captures),
                        Ok(Recognize::FallbackOptions(allowed_methods)) => {
                            let mut response =
                                http::Response::new(crate::output::ResponseBody::default());
                            response
                                .headers_mut()
                                .insert(http::header::ALLOW, allowed_methods.clone());
                            return Ok(Async::Ready(response));
                        }
                        Err(e) => return Err(e.into()),
                    };
                    let route = &self.app.data.routes[pos];
                    debug_assert_eq!(route.id.1, pos);

                    let mut context = AppContext {
                        body: Some(body),
                        is_upgraded: false,
                        route: route.id,
                        captures: params,
                        _priv: (),
                    };

                    if let Some(modifier) = self.app.find_modifier_by_pos(route.id, 0) {
                        BeforeHandle {
                            in_flight: modifier.before_handle(input!(self, &mut context)),
                            context,
                            pos: 0,
                        }
                    } else {
                        Handle {
                            in_flight: route.handler.handle(input!(self, &mut context)),
                            context,
                            pos: 0,
                        }
                    }
                }

                (
                    BeforeHandle {
                        pos, mut context, ..
                    },
                    Polled::BeforeHandle(Some(result)),
                )
                | (
                    Handle {
                        pos, mut context, ..
                    },
                    Polled::Handle(result),
                )
                | (
                    AfterHandle {
                        pos, mut context, ..
                    },
                    Polled::AfterHandle(result),
                ) => match pos.checked_sub(1) {
                    Some(pos) => match self.app.find_modifier_by_pos(context.route_id(), pos) {
                        Some(modifier) => AfterHandle {
                            in_flight: modifier.after_handle(input!(self, &mut context), result),
                            context,
                            pos,
                        },
                        None => break result,
                    },
                    None => break result,
                },

                (
                    BeforeHandle {
                        pos, mut context, ..
                    },
                    Polled::BeforeHandle(None),
                ) => if let Some(modifier) =
                    self.app.find_modifier_by_pos(context.route_id(), pos + 1)
                {
                    BeforeHandle {
                        in_flight: modifier.before_handle(input!(self, &mut context)),
                        context,
                        pos: pos + 1,
                    }
                } else {
                    Handle {
                        in_flight: context
                            .route(&self.app)
                            .handler
                            .handle(input!(self, &mut context)),
                        context,
                        pos: pos + 1,
                    }
                },

                _ => panic!("unexpected state"),
            }
        };

        result.map(Async::Ready)
    }

    fn handle_response(&mut self, mut output: Output) -> Result<Output, Critical> {
        if let Some(ref jar) = self.cookies {
            // append Cookie entries.
            for cookie in jar.delta() {
                output.headers_mut().append(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }

        if let Some(hdrs) = self.response_headers.take() {
            output.headers_mut().extend(hdrs);
        }

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output
                .headers_mut()
                .entry(header::CONTENT_LENGTH)
                .expect("never fails")
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascci.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }

        Ok(output)
    }

    fn handle_error(&mut self, err: Error) -> Result<Output, Critical> {
        self.app
            .data
            .callback
            .on_error(err, callback_context!(self))
    }
}

impl Future for AppFuture {
    type Item = Response<ResponseBody>;
    type Error = Critical;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let output = match self.poll_in_flight() {
            Ok(Async::Ready(output)) => output,
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => {
                self.state = AppFutureState::Done;
                self.handle_error(err)?
            }
        };
        self.handle_response(output).map(Async::Ready)
    }
}

#[derive(Debug)]
pub(crate) struct AppContext {
    body: Option<RequestBody>,
    is_upgraded: bool,
    route: RouteId,
    captures: Option<Captures>,
    _priv: (),
}

impl AppContext {
    pub(crate) fn take_body(&mut self) -> Option<RequestBody> {
        self.body.take()
    }

    pub(crate) fn is_upgraded(&self) -> bool {
        self.is_upgraded
    }

    pub(crate) fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(crate::input::body::UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        if self.is_upgraded() {
            return Err(on_upgrade);
        }
        self.is_upgraded = true;

        let body = self.take_body().expect("The body has already gone");
        crate::rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
    }

    pub(crate) fn route_id(&self) -> RouteId {
        self.route
    }

    fn route<'a>(&self, app: &'a App) -> &'a RouteData {
        app.get_route(self.route)
            .expect("the route ID should be valid")
    }

    pub(crate) fn captures(&self) -> Option<&Captures> {
        self.captures.as_ref()
    }
}
