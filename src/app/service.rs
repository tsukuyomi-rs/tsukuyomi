//! The definition of components for serving an HTTP application by using `App`.

use bytes::Bytes;
use futures::{self, Async, Future};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::sync::Arc;
use std::{fmt, mem};

use error::{CritError, Error};
use future::{self as future_compat, Poll};
use input::{Input, InputParts, RequestBody};
use modifier::{AfterHandle, BeforeHandle};
use output::{Output, ResponseBody};
use server::{Io, ServiceUpgradeExt};
use upgrade::service as upgrade;

use super::{App, AppState};

type HandleFuture = Box<future_compat::Future<Output = Result<Output, Error>> + Send>;

impl App {
    /// Creates a new `AppService` to manage a session.
    pub fn new_service(&self) -> AppService {
        AppService {
            state: self.state.clone(),
            rx: upgrade::new(),
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
    state: Arc<AppState>,
    rx: upgrade::Receiver,
}

impl Service for AppService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Future = AppServiceFuture;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        AppServiceFuture {
            kind: AppServiceFutureKind::Initial(request.map(RequestBody::from_hyp)),
            state: self.state.clone(),
            tx: self.rx.sender(),
        }
    }
}

impl ServiceUpgradeExt<Io> for AppService {
    type Upgrade = AppServiceUpgrade;
    type UpgradeError = CritError;

    fn poll_ready_upgradable(&mut self) -> futures::Poll<(), Self::UpgradeError> {
        self.rx.poll_ready()
    }

    fn upgrade(self, io: Io, read_buf: Bytes) -> Self::Upgrade {
        AppServiceUpgrade {
            inner: self.rx.try_upgrade(io, read_buf),
        }
    }
}

/// A future for managing an incoming HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppServiceFuture {
    kind: AppServiceFutureKind,
    state: Arc<AppState>,
    tx: upgrade::Sender,
}

enum AppServiceFutureKind {
    Initial(Request<RequestBody>),
    BeforeHandle {
        in_flight: BeforeHandle,
        current: usize,
    },
    Handle {
        in_flight: HandleFuture,
        input: Input,
    },
    AfterHandle {
        in_flight: AfterHandle,
        input: Input,
        current: usize,
    },
    Done,
}

impl fmt::Debug for AppServiceFutureKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AppServiceFutureKind::*;
        match *self {
            Initial(ref req) => f.debug_tuple("Initial").field(req).finish(),
            BeforeHandle { .. } => f.debug_tuple("BeforeHandle").finish(),
            Handle { .. } => f.debug_struct("Handle").finish(),
            AfterHandle { .. } => f.debug_tuple("AfterHandle").finish(),
            Done => f.debug_struct("Done").finish(),
        }
    }
}

impl futures::Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        match self.poll_in_flight() {
            Poll::Pending => Ok(futures::Async::NotReady),
            Poll::Ready(Ok((out, cx))) => self.handle_response(out, cx).map(Async::Ready),
            Poll::Ready(Err((err, request))) => self.handle_error(err, request).map(Async::Ready),
        }
    }
}

impl AppServiceFuture {
    fn poll_in_flight(&mut self) -> future_compat::Poll<Result<(Output, InputParts), (Error, Request<()>)>> {
        use self::AppServiceFutureKind::*;

        enum Inner {
            BeforeHandle(Result<Input, (Input, Error)>),
            Handle(Result<Output, Error>),
            AfterHandle(Result<Output, Error>),
            Empty,
        }

        let ret = loop {
            let inner_state = match self.kind {
                Initial(..) => Inner::Empty,
                BeforeHandle { ref mut in_flight, .. } => {
                    Inner::BeforeHandle(ready!(self.state.set(|| in_flight.poll_ready())))
                }
                Handle {
                    ref mut in_flight,
                    ref input,
                } => Inner::Handle(ready!(self.state.set(|| input.set(|| in_flight.poll())))),
                AfterHandle {
                    ref mut in_flight,
                    ref input,
                    ..
                } => Inner::AfterHandle(ready!(self.state.set(|| input.set(|| in_flight.poll_ready())))),
                _ => panic!("unexpected state"),
            };

            match (mem::replace(&mut self.kind, Done), inner_state) {
                (Initial(request), Inner::Empty) => {
                    let (i, params) = match self.state.router().recognize(request.uri().path(), request.method()) {
                        Ok(v) => v,
                        Err(e) => break Err((e, request.map(mem::drop))),
                    };

                    let cx = Input::new(request, i, params, self.state.clone());

                    if let Some(modifier) = self.state.modifiers().get(0) {
                        // Start to applying the modifiers.

                        self.kind = BeforeHandle {
                            in_flight: modifier.before_handle(cx),
                            current: 0,
                        };
                    } else {
                        // No modifiers are registerd. transit to Handle directly.

                        let route = &self.state.router().get_route(i).unwrap();
                        let in_flight = self.state.set(|| route.handle(&cx));
                        self.kind = Handle {
                            in_flight: in_flight,
                            input: cx,
                        };
                    }
                }
                (BeforeHandle { current, .. }, Inner::BeforeHandle(Ok(cx))) => {
                    if let Some(modifier) = self.state.modifiers().get(current) {
                        // Apply the next modifier.
                        self.kind = BeforeHandle {
                            in_flight: modifier.before_handle(cx),
                            current: current + 1,
                        };
                    } else {
                        let i = cx.route_id();
                        let route = &self.state.router().get_route(i).unwrap();
                        let in_flight = self.state.set(|| route.handle(&cx));
                        self.kind = Handle {
                            in_flight: in_flight,
                            input: cx,
                        };
                    }
                }
                (BeforeHandle { .. }, Inner::BeforeHandle(Err((cx, err)))) => {
                    break Err((err, cx.into_parts().request.map(mem::drop)))
                }
                (Handle { input, .. }, Inner::Handle(Ok(out))) => {
                    let current = self.state.modifiers().len();
                    if current > 0 {
                        let modifier = &self.state.modifiers()[current - 1];
                        self.kind = AfterHandle {
                            in_flight: modifier.after_handle(&input, out),
                            input: input,
                            current: current - 1,
                        };
                    } else {
                        break Ok((out, input.into_parts()));
                    }
                }
                (Handle { input, .. }, Inner::Handle(Err(err))) => {
                    break Err((err, input.into_parts().request.map(mem::drop)))
                }
                (AfterHandle { input, current, .. }, Inner::AfterHandle(Ok(output))) => {
                    if current > 0 {
                        let modifier = &self.state.modifiers()[current - 1];
                        self.kind = AfterHandle {
                            in_flight: modifier.after_handle(&input, output),
                            input: input,
                            current: current - 1,
                        };
                    } else {
                        break Ok((output, input.into_parts()));
                    }
                }
                (AfterHandle { input, .. }, Inner::AfterHandle(Err(err))) => {
                    break Err((err, input.into_parts().request.map(mem::drop)))
                }
                _ => panic!("unexpected state"),
            }
        };

        Poll::Ready(ret)
    }

    fn handle_response(&mut self, output: Output, cx: InputParts) -> Result<Response<Body>, CritError> {
        let (mut response, handler) = output.deconstruct();

        cx.cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            self.tx.send(handler, cx.request.map(mem::drop));
        }

        Ok(response.map(ResponseBody::into_hyp))
    }

    fn handle_error(&mut self, err: Error, request: Request<()>) -> Result<Response<Body>, CritError> {
        if let Some(err) = err.as_http_error() {
            let response = self.state.error_handler().handle_error(err, &request)?;
            return Ok(response.map(ResponseBody::into_hyp));
        }
        Err(err.into_critical()
            .expect("unexpected condition in AppServiceFuture::handle_error"))
    }
}

/// A future representing an asynchronous computation after upgrading the protocol
/// from HTTP to another one.
#[must_use = "futures do nothing unless polled"]
pub struct AppServiceUpgrade {
    inner: Result<Box<Future<Item = (), Error = ()> + Send>, Io>,
}

impl fmt::Debug for AppServiceUpgrade {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppServiceUpgrade").finish()
    }
}

impl futures::Future for AppServiceUpgrade {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        match self.inner {
            Ok(ref mut f) => f.poll(),
            Err(ref mut io) => io.shutdown().map_err(mem::drop),
        }
    }
}
