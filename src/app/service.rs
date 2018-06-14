//! The definition of components for serving an HTTP application by using `App`.

use bytes::Bytes;
use futures::{self, Async, Future};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::sync::Arc;
use std::{fmt, mem};

use context::{Context, ContextParts};
use error::{CritError, Error};
use future::{self as future_compat, Poll};
use input::RequestBody;
use output::{Output, ResponseBody};
use server::{Io, ServiceUpgradeExt};
use upgrade::service as upgrade;

use super::{App, AppState};

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
        // TODO: apply middleware
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
    InFlight(
        Context,
        Box<future_compat::Future<Output = Result<Output, Error>> + Send>,
    ),
    Done,
}

impl fmt::Debug for AppServiceFutureKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AppServiceFutureKind::Initial(ref req) => f.debug_tuple("Initial").field(req).finish(),
            AppServiceFutureKind::InFlight(..) => f.debug_struct("InFlight").finish(),
            AppServiceFutureKind::Done => f.debug_struct("Done").finish(),
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
    fn poll_in_flight(&mut self) -> future_compat::Poll<Result<(Output, ContextParts), (Error, Request<()>)>> {
        use self::AppServiceFutureKind::*;
        loop {
            let in_flight_result = match self.kind {
                Initial(..) => None,
                InFlight(ref cx, ref mut f) => Some(ready!(self.state.set(|| cx.set(|| f.poll())))),
                _ => panic!("unexpected state"),
            };

            match (mem::replace(&mut self.kind, Done), in_flight_result) {
                (Initial(request), None) => {
                    let (i, params) = match self.state.router().recognize(request.uri().path(), request.method()) {
                        Ok(v) => v,
                        Err(e) => return Poll::Ready(Err((e, request.map(mem::drop)))),
                    };
                    let route = &self.state.router().get_route(i).unwrap();
                    let cx = Context::new(request, i, params);
                    let in_flight = self.state.set(|| route.handle(&cx));

                    self.kind = InFlight(cx, in_flight);
                }
                (InFlight(cx, _), Some(result)) => {
                    return Poll::Ready(match result {
                        Ok(out) => Ok((out, cx.into_parts())),
                        Err(err) => Err((err, cx.into_parts().request.map(mem::drop))),
                    })
                }
                _ => panic!("unexpected state"),
            }
        }
    }

    fn handle_response(&mut self, output: Output, cx: ContextParts) -> Result<Response<Body>, CritError> {
        let (mut response, handler) = output.deconstruct();

        cx.cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            self.tx.send(handler, cx.request.map(mem::drop));
        }

        Ok(response)
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
