//! The definition of components for serving an HTTP application by using `App`.

use futures::{self, Async, Future, Poll};
use http::header::HeaderValue;
use http::{header, Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::marker::PhantomData;
use std::mem;
use tokio::executor::{DefaultExecutor, Executor};

use error::{CritError, Error};
use input::{Input, InputParts, RequestBody};
use modifier::{AfterHandle, BeforeHandle, Modifier};
use output::{Output, Respond, ResponseBody};
use upgrade::UpgradeContext;

use super::{App, RouteData};

impl App {
    /// Creates a new `AppService` to manage a session.
    pub fn new_service(&self) -> AppService {
        AppService { app: self.clone() }
    }
}

impl NewService for App {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Service = AppService;
    type InitError = CritError;
    type Future = futures::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures::future::ok(self.new_service())
    }
}

/// A `Service` representation of the application, created by `App`.
#[derive(Debug)]
pub struct AppService {
    app: App,
}

impl AppService {
    #[allow(missing_docs)]
    pub fn dispatch_request(&mut self, request: Request<RequestBody>) -> AppServiceFuture {
        AppServiceFuture {
            app: self.app.clone(),
            request: Some(request),
            parts: None,
            status: AppServiceFutureStatus::Start,
        }
    }
}

impl Service for AppService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Future = AppServiceFuture;

    #[inline]
    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        self.dispatch_request(request.map(RequestBody::from_hyp))
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
    Handle { in_flight: Respond, pos: usize },
    AfterHandle { in_flight: AfterHandle, pos: usize },
    Done,
}

impl AppServiceFuture {
    fn get_modifier<'a>(&self, pos: usize, app: &'a App) -> Option<&'a (dyn Modifier + Send + Sync + 'static)> {
        app.modifier(*self.get_route(&self.app)?.modifier_ids.get(pos)?)
    }

    fn get_route<'a>(&self, app: &'a App) -> Option<&'a RouteData> {
        app.route(self.parts.as_ref()?.route)
    }

    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppServiceFutureStatus::*;

        macro_rules! input {
            () => {
                Input {
                    request: self.request
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
                BeforeHandle { ref mut in_flight, .. } => {
                    // FIXME: use result.transpose()
                    Polled::BeforeHandle(match ready!(in_flight.poll_ready(&mut input!())) {
                        Ok(Some(x)) => Some(Ok(x)),
                        Ok(None) => None,
                        Err(e) => Some(Err(e)),
                    })
                }
                Handle { ref mut in_flight, .. } => Polled::Handle(ready!(in_flight.poll_ready(&mut input!()))),
                AfterHandle { ref mut in_flight, .. } => {
                    Polled::AfterHandle(ready!(in_flight.poll_ready(&mut input!())))
                }
                Done => panic!("unexpected state"),
            };

            self.status = match (mem::replace(&mut self.status, Done), polled) {
                (Start, Polled::Empty) => {
                    {
                        let request = self.request.as_ref().expect("This future has already polled");
                        let (pos, params) = match self.app.recognize(request.uri().path(), request.method()) {
                            Ok(r) => r,
                            Err(e) => break Err(e),
                        };
                        let route_id = self.app.inner.routes[pos].id;
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

                (AfterHandle { pos, .. }, Polled::AfterHandle(result)) => match pos.checked_sub(1) {
                    Some(pos) => match self.get_modifier(pos, &self.app) {
                        Some(modifier) => AfterHandle {
                            in_flight: modifier.after_handle(&mut input!(), result),
                            pos,
                        },
                        None => break result,
                    },
                    None => break result,
                },

                _ => panic!("unexpected state"),
            }
        };

        result.map(Async::Ready)
    }

    #[allow(missing_docs)]
    pub fn poll_ready(&mut self, exec: &mut impl Executor) -> Poll<Response<ResponseBody>, CritError> {
        match self.poll_in_flight() {
            Ok(Async::Ready(output)) => self.handle_response(output, exec).map(Async::Ready),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                self.status = AppServiceFutureStatus::Done;
                self.handle_error(err).map(Async::Ready)
            }
        }
    }

    fn handle_response(
        &mut self,
        mut output: Output,
        exec: &mut impl Executor,
    ) -> Result<Response<ResponseBody>, CritError> {
        let (request, body) = {
            let request = self.request.take().expect("This future has already polled.");
            let (parts, body) = request.into_parts();
            (Request::from_parts(parts, ()), body)
        };
        let InputParts {
            cookies,
            locals,
            route,
            params,
            ..
        } = self.parts.take().expect("This future has already polled");

        // append Cookie entries.
        cookies.append_to(output.headers_mut());

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output.headers_mut().entry(header::CONTENT_LENGTH)?.or_insert_with(|| {
                // safety: '0'-'9' is ascci.
                // TODO: more efficient
                unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
            });
        }

        // spawn the upgrade task.
        if let (Some(body), Some(mut upgrade)) = body.deconstruct() {
            if output.status() == StatusCode::SWITCHING_PROTOCOLS {
                let app = self.app.clone();
                exec.spawn(Box::new(
                    body.on_upgrade()
                        .map_err(|e| error!("upgrade error: {}", e))
                        .and_then(move |upgraded| {
                            upgrade(UpgradeContext {
                                io: upgraded,
                                request,
                                locals,
                                route,
                                params,
                                app,
                                _marker: PhantomData,
                            })
                        }),
                )).map_err(|_| format_err!("failed spawn the upgrade task").compat())?;
            }
        }

        Ok(output)
    }

    fn handle_error(&mut self, err: Error) -> Result<Response<ResponseBody>, CritError> {
        let request = self.request
            .take()
            .expect("This future has already polled")
            .map(mem::drop);
        drop(self.parts.take());

        if let Some(err) = err.as_http_error() {
            let response = self.app.error_handler().handle_error(err, &request)?;
            return Ok(response);
        }

        Err(err.into_critical().unwrap())
    }
}

impl futures::Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        // FIXME: use futures::task::Context::executor() instead.
        let mut exec = DefaultExecutor::current();
        self.poll_ready(&mut exec)
            .map(|x| x.map(|response| response.map(ResponseBody::into_hyp)))
    }
}
