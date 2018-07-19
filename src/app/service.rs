//! The definition of components for serving an HTTP application by using `App`.

use futures::future::lazy;
use futures::{self, Async, Future, Poll};
use http::header::HeaderValue;
use http::{header, Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::mem;
use tokio;

use error::{CritError, Error};
use handler::Handle;
use input::{Input, InputParts, RequestBody};
use modifier::{AfterHandle, BeforeHandle, Modifier};
use output::upgrade::UpgradeContext;
use output::{Output, ResponseBody};

use super::App;

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

#[derive(Debug)]
enum AppServiceFutureStatus {
    Start,
    Recognized,
    BeforeHandle { in_flight: BeforeHandle, pos: usize },
    Handle { in_flight: Handle, pos: usize },
    AfterHandle { in_flight: AfterHandle, pos: usize },
    Done,
}

impl AppServiceFuture {
    fn get_modifier<'a>(&self, pos: usize, app: &'a App) -> Option<&'a (dyn Modifier + Send + Sync + 'static)> {
        let parts = self.parts.as_ref()?;
        let &id = self.app.endpoint(parts.recognize.endpoint_id)?.modifier_ids.get(pos)?;
        app.modifier(id)
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

        enum Polled {
            BeforeHandle(Option<Result<Output, Error>>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
            Empty,
        }

        let result = loop {
            let polled = match self.status {
                Start | Recognized => Polled::Empty,
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
                    let request = self.request.as_ref().expect("This future has already polled");
                    let recognize = match self.app.recognize(request.uri().path(), request.method()) {
                        Ok(recognize) => recognize,
                        Err(e) => break Err(e),
                    };
                    self.parts = Some(InputParts::new(recognize));
                    Recognized
                }

                (Recognized, Polled::Empty) => match self.get_modifier(0, &self.app) {
                    Some(modifier) => BeforeHandle {
                        in_flight: modifier.before_handle(&mut input!()),
                        pos: 0,
                    },
                    None => {
                        let mut input = input!();
                        let endpoint = input.endpoint_in(&self.app);
                        Handle {
                            in_flight: endpoint.handler.handle(&mut input),
                            pos: 0,
                        }
                    }
                },

                (BeforeHandle { pos, .. }, Polled::BeforeHandle(result)) => match result {
                    Some(result) => match pos.checked_sub(1) {
                        Some(pos) => match self.get_modifier(pos, &self.app) {
                            Some(modifier) => AfterHandle {
                                in_flight: modifier.after_handle(&mut input!(), result),
                                pos: pos,
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
                        None => {
                            let mut input = input!();
                            let endpoint = input.endpoint_in(&self.app);
                            Handle {
                                in_flight: endpoint.handler.handle(&mut input),
                                pos: pos + 1,
                            }
                        }
                    },
                },

                (Handle { pos, .. }, Polled::Handle(result)) => match pos.checked_sub(1) {
                    Some(pos) => match self.get_modifier(pos, &self.app) {
                        Some(modifier) => AfterHandle {
                            in_flight: modifier.after_handle(&mut input!(), result),
                            pos: pos,
                        },
                        None => break result,
                    },
                    None => break result,
                },

                (AfterHandle { pos, .. }, Polled::AfterHandle(result)) => match pos.checked_sub(1) {
                    Some(pos) => match self.get_modifier(pos, &self.app) {
                        Some(modifier) => AfterHandle {
                            in_flight: modifier.after_handle(&mut input!(), result),
                            pos: pos,
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
    pub fn poll_ready(&mut self) -> Poll<Response<ResponseBody>, CritError> {
        match self.poll_in_flight() {
            Ok(Async::Ready(output)) => self.handle_response(output).map(Async::Ready),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => {
                self.status = AppServiceFutureStatus::Done;
                self.handle_error(err).map(Async::Ready)
            }
        }
    }

    fn handle_response(&mut self, output: Output) -> Result<Response<ResponseBody>, CritError> {
        let (mut response, handler) = output.deconstruct();

        let parts = self.parts.take().expect("This future has already polled");
        let InputParts { cookies, .. } = parts;

        cookies.append_to(response.headers_mut());

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = response.body().content_length() {
            response
                .headers_mut()
                .entry(header::CONTENT_LENGTH)?
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascci.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

            let mut request = self.request.take().expect("This future has already polled.");
            let on_upgrade = request
                .body_mut()
                .on_upgrade()
                .ok_or_else(|| format_err!("The request body has already gone").compat())?;
            let request = request.map(mem::drop);

            tokio::spawn(lazy(move || {
                on_upgrade.map_err(|_| error!("")).and_then(|upgraded| {
                    let cx = UpgradeContext {
                        io: upgraded,
                        request: request,
                        _priv: (),
                    };
                    handler.upgrade(cx)
                })
            }));
        }

        Ok(response)
    }

    fn handle_error(&mut self, err: Error) -> Result<Response<ResponseBody>, CritError> {
        if let Some(err) = err.as_http_error() {
            let request = self.request
                .take()
                .expect("This future has already polled")
                .map(mem::drop);
            let response = self.app.error_handler().handle_error(err, &request)?;
            return Ok(response);
        }
        Err(err.into_critical()
            .expect("unexpected condition in AppServiceFuture::handle_error"))
    }
}

impl futures::Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        self.poll_ready()
            .map(|x| x.map(|response| response.map(ResponseBody::into_hyp)))
    }
}
