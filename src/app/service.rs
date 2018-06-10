use bytes::Bytes;
use failure;
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
        // TODO: apply middleware

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
            in_flight: None,
            context: Some(Context::new(request.map(RequestBody::from_hyp), self.state.clone())),
            tx: self.rx.sender(),
        }
    }
}

impl ServiceUpgradeExt<Io> for AppService {
    type Upgrade = AppServiceUpgrade;
    type UpgradeError = failure::Error;

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
pub struct AppServiceFuture {
    in_flight: Option<Box<Future<Item = Output, Error = Error> + Send>>,
    context: Option<Context>,
    tx: upgrade::Sender,
}

impl fmt::Debug for AppServiceFuture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppServiceFuture")
            .field("in_flight", &"<a boxed future>")
            .field("context", &self.context)
            .field("tx", &self.tx)
            .finish()
    }
}

impl Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_in_flight() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(out)) => {
                let cx = self.pop_context();
                Ok(Async::Ready(self.handle_response(out, cx.into_parts())))
            }
            Err(err) => {
                let cx = self.pop_context();
                self.handle_error(err, cx.into_parts()).map(Into::into)
            }
        }
    }
}

impl AppServiceFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        if self.in_flight.is_none() {
            let cx = self.context.as_mut().unwrap();
            let (i, params) = cx.state().router().recognize(cx.uri().path(), cx.method())?;
            cx.set_route(i, params);
            let route = cx.state().router().get_route(i).unwrap();
            self.in_flight = Some(route.handle(&cx));
        }

        let in_flight = self.in_flight.as_mut().unwrap();
        let cx = self.context
            .as_ref()
            .expect("AppServiceFuture has already resolved/rejected");
        cx.set(|| in_flight.poll())
    }

    fn pop_context(&mut self) -> Context {
        self.context
            .take()
            .expect("AppServiceFuture has already resolved/rejected")
    }

    fn handle_response(&mut self, output: Output, cx: ContextParts) -> Response<Body> {
        let (mut response, handler) = output.deconstruct();

        // TODO: apply middlewares

        #[cfg(feature = "session")]
        cx.cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            self.tx.send(handler, cx.request.map(mem::drop));
        }

        response
    }

    fn handle_error(&mut self, err: Error, cx: ContextParts) -> Result<Response<Body>, CritError> {
        if let Some(err) = err.as_http_error() {
            let request = cx.request.map(mem::drop);
            let response = cx.state.error_handler().handle_error(err, &request)?;
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
