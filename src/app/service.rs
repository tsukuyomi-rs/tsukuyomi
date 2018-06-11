use bytes::Bytes;
use futures::{future, Async, Future, Poll};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::sync::Arc;
use std::{fmt, mem};

use context::{Context, ContextParts};
use error::{CritError, Error};
use input::RequestBody;
use output::{Output, ResponseBody};
use server::{Io, ServiceUpgradeExt};
use upgrade::service as upgrade;

use super::{App, AppState};

impl App {
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
    type Future = future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        future::ok(self.new_service())
    }
}

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
            context: None,
            state: self.state.clone(),
            tx: self.rx.sender(),
        }
    }
}

impl ServiceUpgradeExt<Io> for AppService {
    type Upgrade = AppServiceUpgrade;
    type UpgradeError = CritError;

    fn poll_ready_upgradable(&mut self) -> Poll<(), Self::UpgradeError> {
        self.rx.poll_ready()
    }

    fn upgrade(self, io: Io, read_buf: Bytes) -> Self::Upgrade {
        AppServiceUpgrade {
            inner: self.rx.try_upgrade(io, read_buf),
        }
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppServiceFuture {
    kind: AppServiceFutureKind,
    context: Option<Context>,
    state: Arc<AppState>,
    tx: upgrade::Sender,
}

enum AppServiceFutureKind {
    Initial(Request<RequestBody>),
    InFlight(Box<Future<Item = Output, Error = Error> + Send>),
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

impl Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = ready!(self.poll_in_flight());
        let cx = self.context
            .take()
            .expect("AppServiceFuture has already resolved/rejected")
            .into_parts();
        match result {
            Ok(out) => Ok(Async::Ready(self.handle_response(out, cx))),
            Err(err) => {
                let request = cx.request.map(mem::drop);
                self.handle_error(err, request).map(Async::Ready)
            }
        }
    }
}

impl AppServiceFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppServiceFutureKind::*;
        loop {
            match (&mut self.kind, &self.context) {
                (Initial(..), ..) => {}
                (InFlight(ref mut f), Some(ref cx)) => return self.state.set(|| cx.set(|| f.poll())),
                _ => panic!("unexpected state"),
            }

            if let Initial(request) = mem::replace(&mut self.kind, Done) {
                let (i, params) = self.state.router().recognize(request.uri().path(), request.method())?;
                let cx = Context::new(request, i, params);
                let in_flight = self.state.router().get_route(i).unwrap().handle(&cx);

                self.kind = InFlight(in_flight);
                self.context = Some(cx);
            }
        }
    }

    fn handle_response(&mut self, output: Output, cx: ContextParts) -> Response<Body> {
        let (mut response, handler) = output.deconstruct();

        #[cfg(feature = "session")]
        cx.cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            self.tx.send(handler, cx.request.map(mem::drop));
        }

        response
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

#[must_use = "futures do nothing unless polled"]
pub struct AppServiceUpgrade {
    inner: Result<Box<Future<Item = (), Error = ()> + Send>, Io>,
}

impl fmt::Debug for AppServiceUpgrade {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppServiceUpgrade").finish()
    }
}

impl Future for AppServiceUpgrade {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Ok(ref mut f) => f.poll(),
            Err(ref mut io) => io.shutdown().map_err(mem::drop),
        }
    }
}
