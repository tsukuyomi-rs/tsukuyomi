//! The definition of components for serving an HTTP application by using `App`.

use futures::{Async, Future, Poll};
use http::header::HeaderValue;
use http::{header, Method, Request, Response, StatusCode};
use std::mem;
use tower_service::{NewService, Service};

use crate::error::{Error, HttpError};
use crate::handler::Handle;
use crate::input::{Input, InputParts, RequestBody};
use crate::modifier::{AfterHandle, BeforeHandle, Modifier};
use crate::output::{Output, ResponseBody};
use crate::recognizer::captures::Captures;
use crate::server::server::CritError;
use crate::server::service::http::Payload;

use super::{App, ModifierId, RouteData, RouteId, ScopeId};

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
    pub(super) fn recognize(
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
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct AppService {
    app: App,
}

impl Service for AppService {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = CritError;
    type Future = AppServiceFuture;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    #[inline]
    fn call(&mut self, request: Self::Request) -> Self::Future {
        AppServiceFuture {
            app: self.app.clone(),
            request: Some(request),
            parts: None,
            status: AppServiceFutureStatus::Start,
        }
    }
}

/// A future for managing an incoming HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppServiceFuture {
    app: App,
    request: Option<Request<RequestBody>>,
    parts: Option<InputParts>,
    status: AppServiceFutureStatus,
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum AppServiceFutureStatus {
    Start,
    BeforeHandle { in_flight: BeforeHandle, pos: usize },
    Handle { in_flight: Handle, pos: usize },
    AfterHandle { in_flight: AfterHandle, pos: usize },
    Done,
}

impl AppServiceFuture {
    fn get_modifier<'a>(
        &self,
        pos: usize,
        app: &'a App,
    ) -> Option<&'a (dyn Modifier + Send + Sync + 'static)> {
        let &id = self.get_route(&self.app)?.modifier_ids.get(pos)?;
        match id {
            ModifierId(ScopeId::Global, pos) => app.data.modifiers.get(pos).map(|m| &**m),
            ModifierId(ScopeId::Local(id), pos) => {
                app.data.scopes.get(id)?.modifiers.get(pos).map(|m| &**m)
            }
        }
    }

    fn get_route<'a>(&self, app: &'a App) -> Option<&'a RouteData> {
        let RouteId(_, pos) = self.parts.as_ref()?.route;
        app.data.routes.get(pos)
    }

    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppServiceFutureStatus::*;

        macro_rules! input {
            () => {
                Input {
                    request: self
                        .request
                        .as_mut()
                        .expect("This future has already polled"),
                    parts: self.parts.as_mut().expect("This future has already polled"),
                    app: &self.app,
                }
            };
        }

        macro_rules! ready {
            ($e:expr) => {
                match $e {
                    Ok(::futures::Async::Ready(x)) => Ok(x),
                    Ok(::futures::Async::NotReady) => return Ok(::futures::Async::NotReady),
                    Err(e) => Err(e),
                }
            };
        }

        #[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
        enum Polled {
            BeforeHandle(Option<Result<Output, Error>>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
            Empty,
        }

        let result = loop {
            let polled = match self.status {
                Start => Polled::Empty,
                BeforeHandle {
                    ref mut in_flight, ..
                } => {
                    // FIXME: use result.transpose()
                    Polled::BeforeHandle(match ready!(in_flight.poll_ready(&mut input!())) {
                        Ok(Some(x)) => Some(Ok(x)),
                        Ok(None) => None,
                        Err(e) => Some(Err(e)),
                    })
                }
                Handle {
                    ref mut in_flight, ..
                } => Polled::Handle(ready!(in_flight.poll_ready(&mut input!()))),
                AfterHandle {
                    ref mut in_flight, ..
                } => Polled::AfterHandle(ready!(in_flight.poll_ready(&mut input!()))),
                Done => panic!("unexpected state"),
            };

            self.status = match (mem::replace(&mut self.status, Done), polled) {
                (Start, Polled::Empty) => {
                    {
                        let request = self
                            .request
                            .as_ref()
                            .expect("This future has already polled");
                        let (pos, params) =
                            match self.app.recognize(request.uri().path(), request.method()) {
                                Ok(r) => r,
                                Err(e) => break Err(e.into()),
                            };
                        let route_id = self.app.data.routes[pos].id;
                        debug_assert_eq!(route_id.1, pos);
                        self.parts = Some(InputParts::new(route_id, params));
                    }

                    match self.get_modifier(0, &self.app) {
                        Some(modifier) => BeforeHandle {
                            in_flight: modifier.before_handle(&mut input!()),
                            pos: 0,
                        },
                        None => match self.get_route(&self.app) {
                            Some(endpoint) => Handle {
                                in_flight: endpoint.handler.handle(&mut input!()),
                                pos: 0,
                            },
                            None => panic!(""),
                        },
                    }
                }

                (BeforeHandle { pos, .. }, Polled::BeforeHandle(result)) => match result {
                    Some(result) => match pos.checked_sub(1) {
                        Some(pos) => match self.get_modifier(pos, &self.app) {
                            Some(modifier) => AfterHandle {
                                in_flight: modifier.after_handle(&mut input!(), result),
                                pos,
                            },
                            None => break result,
                        },
                        None => break result,
                    },
                    None => match self.get_modifier(pos + 1, &self.app) {
                        Some(modifier) => BeforeHandle {
                            in_flight: modifier.before_handle(&mut input!()),
                            pos: pos + 1,
                        },
                        None => match self.get_route(&self.app) {
                            Some(endpoint) => Handle {
                                in_flight: endpoint.handler.handle(&mut input!()),
                                pos: pos + 1,
                            },
                            None => panic!(""),
                        },
                    },
                },

                (Handle { pos, .. }, Polled::Handle(result)) => match pos.checked_sub(1) {
                    Some(pos) => match self.get_modifier(pos, &self.app) {
                        Some(modifier) => AfterHandle {
                            in_flight: modifier.after_handle(&mut input!(), result),
                            pos,
                        },
                        None => break result,
                    },
                    None => break result,
                },

                (AfterHandle { pos, .. }, Polled::AfterHandle(result)) => {
                    match pos.checked_sub(1) {
                        Some(pos) => match self.get_modifier(pos, &self.app) {
                            Some(modifier) => AfterHandle {
                                in_flight: modifier.after_handle(&mut input!(), result),
                                pos,
                            },
                            None => break result,
                        },
                        None => break result,
                    }
                }

                _ => panic!("unexpected state"),
            }
        };

        result.map(Async::Ready)
    }

    fn handle_response(&mut self, mut output: Output) -> Result<Response<ResponseBody>, CritError> {
        let _request = self
            .request
            .take()
            .expect("This future has already polled.");
        let InputParts { cookies, .. } = self.parts.take().expect("This future has already polled");

        // append Cookie entries.
        cookies.append_to(output.headers_mut());

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

impl Future for AppServiceFuture {
    type Item = Response<ResponseBody>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_in_flight() {
            Ok(Async::Ready(output)) => self.handle_response(output).map(Async::Ready),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                self.status = AppServiceFutureStatus::Done;
                let request = self.request.take().expect("This future has already polled");
                drop(self.parts.take());

                self.app
                    .data
                    .error_handler
                    .handle_error(err, &request)
                    .map(Async::Ready)
            }
        }
    }
}
