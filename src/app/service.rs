//! The definition of components for serving an HTTP application by using `App`.

use futures::future::lazy;
use futures::{self, Async, Future, Poll};
use http::header::HeaderValue;
use http::{header, Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::mem;
use std::sync::Arc;
use tokio;

use error::{CritError, Error};
use handler::Handle;
use input::{Input, InputParts, RequestBody};
use modifier::{AfterHandle, BeforeHandle};
use output::upgrade::UpgradeContext;
use output::{Output, ResponseBody};

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
            state: AppServiceFutureState::Initial,
            input: Some(Input {
                parts: InputParts::new(request),
                state: self.global.clone(),
            }),
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
    input: Option<Input>,
    global: Arc<AppState>,
}

#[derive(Debug)]
enum AppServiceFutureState {
    Initial,
    BeforeHandle(BeforeHandle, usize),
    Handle(Handle),
    AfterHandle(AfterHandle, usize),
    Done,
}

impl AppServiceFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppServiceFutureState::*;

        let input = self.input.as_mut().expect("This future has already polled");
        let global = &*self.global;

        loop {
            let output = match self.state {
                Initial => None,
                BeforeHandle(ref mut in_flight, ..) => try_ready!(in_flight.poll_ready(input)),
                Handle(ref mut in_flight) => Some(try_ready!(in_flight.poll_ready(input))),
                AfterHandle(ref mut in_flight, ..) => Some(try_ready!(in_flight.poll_ready(input))),
                _ => panic!("unexpected state"),
            };

            match (mem::replace(&mut self.state, Done), output) {
                (Initial, None) => {
                    if let Some(modifier) = global.modifiers().get(0) {
                        self.state = BeforeHandle(modifier.before_handle(input), 1);
                    } else {
                        self.state = Handle(global.handle(input)?);
                    }
                }

                (BeforeHandle(_, current), Some(output)) => {
                    if current <= 1 {
                        break Ok(Async::Ready(output));
                    }
                    let modifier = &global.modifiers()[current - 2];
                    self.state = AfterHandle(modifier.after_handle(input, output), current - 2);
                }

                (BeforeHandle(_, current), None) => {
                    if let Some(modifier) = global.modifiers().get(current) {
                        self.state = BeforeHandle(modifier.before_handle(input), current + 1);
                    } else {
                        self.state = Handle(global.handle(input)?);
                    }
                }

                (Handle(..), Some(output)) => {
                    let current = input.state.modifiers().len();
                    if current == 0 {
                        break Ok(Async::Ready(output));
                    }
                    let modifier = &global.modifiers()[current - 1];
                    self.state = AfterHandle(modifier.after_handle(input, output), current - 1);
                }

                (AfterHandle(_, current), Some(output)) => {
                    if current == 0 {
                        break Ok(Async::Ready(output));
                    }
                    let modifier = &global.modifiers()[current - 1];
                    self.state = AfterHandle(modifier.after_handle(input, output), current - 1);
                }

                _ => panic!("unexpected state"),
            }
        }
    }
}

impl AppServiceFuture {
    #[allow(missing_docs)]
    pub fn poll_ready(&mut self) -> Poll<Response<ResponseBody>, CritError> {
        let polled = match self.poll_in_flight() {
            Ok(Async::Ready(output)) => Ok(output),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(err) => Err(err),
        };
        self.state = AppServiceFutureState::Done;

        let input = self.input.take().expect("This future has already polled");
        match polled {
            Ok(output) => self.handle_response(output, input).map(Async::Ready),
            Err(err) => self.handle_error(err, input).map(Async::Ready),
        }
    }

    fn handle_response(&mut self, output: Output, input: Input) -> Result<Response<ResponseBody>, CritError> {
        let (mut response, handler) = output.deconstruct();
        let InputParts {
            cookies, mut request, ..
        } = input.parts;

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

    fn handle_error(&mut self, err: Error, input: Input) -> Result<Response<ResponseBody>, CritError> {
        let request = input.parts.request.map(mem::drop);
        let global = input.state;

        if let Some(err) = err.as_http_error() {
            let response = global.error_handler().handle_error(err, &request)?;
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
