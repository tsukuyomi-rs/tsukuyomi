//! The definition of components for serving an HTTP application by using `App`.

use futures::future::lazy;
use futures::{self, Future as _Future};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::mem;
use std::sync::Arc;
use tokio;

use error::{CritError, Error};
use future::Poll;
use input::{Input, InputParts, RequestBody};
use modifier::{AfterHandle, BeforeHandle};
use output::upgrade::UpgradeContext;
use output::{Output, ResponseBody};
use router::Handle;

use super::{App, AppState};

impl App {
    /// Creates a new `AppService` to manage a session.
    pub fn new_service(&self) -> AppService {
        AppService {
            global: self.global.clone(),
        }
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
    global: Arc<AppState>,
}

impl AppService {
    #[allow(missing_docs)]
    pub fn dispatch_request(&mut self, request: Request<RequestBody>) -> AppServiceFuture {
        AppServiceFuture {
            state: AppServiceFutureState::Initial(request),
            global: self.global.clone(),
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
    state: AppServiceFutureState,
    global: Arc<AppState>,
}

#[derive(Debug)]
enum AppServiceFutureState {
    Initial(Request<RequestBody>),
    BeforeHandle {
        in_flight: BeforeHandle,
        input: Input,
        current: usize,
        route_id: usize,
    },
    Handle {
        in_flight: Handle,
        input: Input,
    },
    AfterHandle {
        in_flight: AfterHandle,
        input: Input,
        current: usize,
    },
    Done,
}

impl AppServiceFutureState {
    fn poll_in_flight(&mut self, global: &AppState) -> Poll<Result<(Output, Input), (Error, Request<()>)>> {
        use self::AppServiceFutureState::*;

        enum Polled {
            BeforeHandle(Result<(), Error>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
        }

        let ret = loop {
            let polled = match self {
                Initial(..) => None,
                BeforeHandle {
                    ref mut in_flight,
                    ref mut input,
                    ..
                } => Some(Polled::BeforeHandle(ready!(in_flight.poll_ready(input)))),
                Handle {
                    ref mut in_flight,
                    ref mut input,
                } => Some(Polled::Handle(ready!(in_flight.poll_ready(input)))),
                AfterHandle {
                    ref mut in_flight,
                    ref mut input,
                    ..
                } => Some(Polled::AfterHandle(ready!(in_flight.poll_ready(input)))),
                _ => panic!("unexpected state"),
            };

            match (mem::replace(self, Done), polled) {
                (Initial(request), None) => {
                    let (i, params) = match global.router().recognize(request.uri().path(), request.method()) {
                        Ok(v) => v,
                        Err(e) => break Err((e, request.map(mem::drop))),
                    };

                    let mut input = Input::new(request, i, params);

                    if let Some(modifier) = global.modifiers().get(0) {
                        let in_flight = modifier.before_handle(&mut input);
                        *self = BeforeHandle {
                            in_flight: in_flight,
                            input: input,
                            current: 0,
                            route_id: i,
                        };
                    } else {
                        let route = &global.router()[i];
                        let in_flight = route.handle(&mut input);
                        *self = Handle {
                            in_flight: in_flight,
                            input: input,
                        };
                    }
                }

                (
                    BeforeHandle {
                        current,
                        mut input,
                        route_id,
                        ..
                    },
                    Some(Polled::BeforeHandle(Ok(()))),
                ) => {
                    if let Some(modifier) = global.modifiers().get(current) {
                        let in_flight = modifier.before_handle(&mut input);
                        *self = BeforeHandle {
                            in_flight: in_flight,
                            input: input,
                            current: current + 1,
                            route_id: route_id,
                        };
                    } else {
                        let route = &global.router()[route_id];
                        let in_flight = route.handle(&mut input);
                        *self = Handle {
                            in_flight: in_flight,
                            input: input,
                        };
                    }
                }

                (Handle { mut input, .. }, Some(Polled::Handle(Ok(output)))) => {
                    let current = global.modifiers().len();
                    if current > 0 {
                        let modifier = &global.modifiers()[current - 1];
                        let in_flight = modifier.after_handle(&mut input, output);
                        *self = AfterHandle {
                            in_flight: in_flight,
                            input: input,
                            current: current - 1,
                        };
                    } else {
                        break Ok((output, input));
                    }
                }

                (AfterHandle { mut input, current, .. }, Some(Polled::AfterHandle(Ok(output)))) => {
                    if current > 0 {
                        let modifier = &global.modifiers()[current - 1];
                        let in_flight = modifier.after_handle(&mut input, output);
                        *self = AfterHandle {
                            in_flight: in_flight,
                            input: input,
                            current: current - 1,
                        };
                    } else {
                        break Ok((output, input));
                    }
                }

                | (BeforeHandle { input, .. }, Some(Polled::BeforeHandle(Err(err))))
                | (Handle { input, .. }, Some(Polled::Handle(Err(err))))
                | (AfterHandle { input, .. }, Some(Polled::AfterHandle(Err(err)))) => {
                    break Err((err, input.into_parts().request.map(mem::drop)))
                }

                _ => panic!("unexpected state"),
            }
        };

        Poll::Ready(ret)
    }
}

impl AppServiceFuture {
    #[allow(missing_docs)]
    pub fn poll_ready(&mut self) -> Poll<Result<Response<ResponseBody>, CritError>> {
        match {
            let state = &mut self.state;
            let global = &mut self.global;
            ready!(global.with_set(|| state.poll_in_flight(global)))
        } {
            Ok((out, input)) => Poll::Ready(self.handle_response(out, input.into_parts())),
            Err((err, request)) => Poll::Ready(self.handle_error(err, request)),
        }
    }

    fn handle_response(&mut self, output: Output, input: InputParts) -> Result<Response<ResponseBody>, CritError> {
        let (mut response, handler) = output.deconstruct();
        let InputParts {
            cookies, mut request, ..
        } = input;

        cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

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

    fn handle_error(&mut self, err: Error, request: Request<()>) -> Result<Response<ResponseBody>, CritError> {
        if let Some(err) = err.as_http_error() {
            let response = self.global.error_handler().handle_error(err, &request)?;
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
            .map_ok(|response| response.map(ResponseBody::into_hyp))
            .into()
    }
}
