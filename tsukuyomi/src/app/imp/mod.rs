//! The definition of components for serving an HTTP application by using `App`.

pub(crate) mod input;

use cookie::{Cookie, CookieJar};
use futures::{Async, Future, Poll};
use http::header::{HeaderMap, HeaderValue};
use http::{header, Method, Request, Response, StatusCode};
use std::mem;
use tower_service::{NewService, Service};

use crate::error::{Error, HttpError};
use crate::input::{Input, RequestBody};
use crate::local_map::LocalMap;
use crate::output::{Output, ResponseBody};
use crate::recognizer::Captures;
use crate::server::service::http::Payload;
use crate::server::CritError;

use super::handler::AsyncResult;
use super::{App, RouteData, RouteId};

macro_rules! ready {
    ($e:expr) => {
        match $e {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(x)) => Ok(x),
            Err(e) => Err(e),
        }
    };
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

impl App {
    #[doc(hidden)]
    pub fn recognize(
        &self,
        path: &str,
        method: &Method,
    ) -> Result<(usize, Option<Captures>), RecognizeError> {
        let (i, params) = self
            .data
            .recognizer
            .recognize(path)
            .ok_or_else(|| RecognizeError::NotFound)?;

        let methods = &self.data.route_ids[i];
        match methods.get(method) {
            Some(&i) => Ok((i, params)),
            None if self.data.config.fallback_head && *method == Method::HEAD => {
                match methods.get(&Method::GET) {
                    Some(&i) => Ok((i, params)),
                    None => Err(RecognizeError::MethodNotAllowed),
                }
            }
            None => Err(RecognizeError::MethodNotAllowed),
        }
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = CritError;
    type Service = AppService;
    type InitError = CritError;
    type Future = futures::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures::future::ok(AppService { app: self.clone() })
    }
}

/// A `Service` representation of the application, created by `App`.
#[derive(Debug)]
pub struct AppService {
    app: App,
}

impl Service for AppService {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = CritError;
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
            app: self.app.clone(),
            request: Request::from_parts(parts, ()),
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

impl AppFuture {
    fn poll_in_flight(&mut self) -> Poll<(Output, AppContext), Error> {
        use self::AppFutureState::*;

        macro_rules! input {
            ($context:expr) => {
                &mut Input::new(&self.request, &self.app, $context)
            };
        }

        #[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
        enum Polled {
            BeforeHandle(Option<Result<Output, Error>>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
            Empty,
        }

        let (result, context) = loop {
            let polled = match self.state {
                Init(..) => Polled::Empty,
                BeforeHandle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => {
                    // FIXME: use result.transpose()
                    Polled::BeforeHandle(match ready!(in_flight.poll_ready(input!(context))) {
                        Ok(Some(x)) => Some(Ok(x)),
                        Ok(None) => None,
                        Err(e) => Some(Err(e)),
                    })
                }
                Handle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => Polled::Handle(ready!(in_flight.poll_ready(input!(context)))),
                AfterHandle {
                    ref mut in_flight,
                    ref mut context,
                    ..
                } => Polled::AfterHandle(ready!(in_flight.poll_ready(input!(context)))),
                Done => panic!("unexpected state"),
            };

            self.state = match (mem::replace(&mut self.state, Done), polled) {
                (Init(body), Polled::Empty) => {
                    let (pos, params) = match self
                        .app
                        .recognize(self.request.uri().path(), self.request.method())
                    {
                        Ok(r) => r,
                        Err(e) => return Err(e.into()),
                    };
                    let route = &self.app.data.routes[pos];
                    debug_assert_eq!(route.id.1, pos);

                    let mut context = AppContext {
                        body: Some(body),
                        is_upgraded: false,
                        route: route.id,
                        captures: params,
                        cookies: None,
                        locals: LocalMap::default(),
                        _priv: (),
                    };

                    if let Some(modifier) = self.app.find_modifier_by_pos(route.id, 0) {
                        BeforeHandle {
                            in_flight: modifier.before_handle(input!(&mut context)),
                            context,
                            pos: 0,
                        }
                    } else {
                        Handle {
                            in_flight: route.handler.handle(input!(&mut context)),
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
                            in_flight: modifier.after_handle(input!(&mut context), result),
                            context,
                            pos,
                        },
                        None => break (result, context),
                    },
                    None => break (result, context),
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
                        in_flight: modifier.before_handle(input!(&mut context)),
                        context,
                        pos: pos + 1,
                    }
                } else {
                    Handle {
                        in_flight: context
                            .route(&self.app)
                            .handler
                            .handle(input!(&mut context)),
                        context,
                        pos: pos + 1,
                    }
                },

                _ => panic!("unexpected state"),
            }
        };

        result.map(|output| Async::Ready((output, context)))
    }

    fn handle_response(
        &mut self,
        mut output: Output,
        context: &AppContext,
    ) -> Result<Response<ResponseBody>, CritError> {
        // append Cookie entries.
        context.append_cookies(output.headers_mut());

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output
                .headers_mut()
                .entry(header::CONTENT_LENGTH)?
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascci.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }

        Ok(output)
    }
}

impl Future for AppFuture {
    type Item = Response<ResponseBody>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_in_flight() {
            Ok(Async::Ready((output, context))) => {
                self.handle_response(output, &context).map(Async::Ready)
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                self.state = AppFutureState::Done;
                self.app
                    .data
                    .error_handler
                    .handle_error(err, &self.request)
                    .map(Async::Ready)
            }
        }
    }
}

#[derive(Debug)]
struct AppContext {
    body: Option<RequestBody>,
    is_upgraded: bool,
    route: RouteId,
    captures: Option<Captures>,
    locals: LocalMap,
    cookies: Option<CookieJar>,
    _priv: (),
}

impl AppContext {
    fn route_id(&self) -> RouteId {
        self.route
    }

    fn route<'a>(&self, app: &'a App) -> &'a RouteData {
        app.get_route(self.route)
            .expect("the route ID should be valid")
    }

    fn init_cookie_jar(&mut self, h: &HeaderMap) -> Result<&mut CookieJar, Error> {
        if let Some(ref mut jar) = self.cookies {
            return Ok(jar);
        }

        let mut jar = CookieJar::new();

        for raw in h.get_all(header::COOKIE) {
            let raw_s = raw.to_str().map_err(crate::error::bad_request)?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)
                    .map_err(crate::error::bad_request)?
                    .into_owned();
                jar.add_original(cookie);
            }
        }

        Ok(self.cookies.get_or_insert(jar))
    }

    fn append_cookies(&self, h: &mut HeaderMap) {
        if let Some(ref jar) = self.cookies {
            for cookie in jar.delta() {
                h.insert(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }
    }
}
