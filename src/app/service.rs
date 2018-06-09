use bytes::Bytes;
use futures::{future, Async, Future, Poll};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::sync::Arc;
use std::{fmt, mem};

use context::Context;
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
        let mut cx = Context::new(request.map(RequestBody::from_hyp), self.state.clone());

        // TODO: apply middleware

        let in_flight = self.state.router().handle(&mut cx);
        AppServiceFuture {
            in_flight: in_flight,
            context: Some(cx),
            tx: self.rx.sender(),
        }
    }
}

impl ServiceUpgradeExt<Io> for AppService {
    type Upgrade = Box<Future<Item = (), Error = ()> + Send>;
    type UpgradeError = ::failure::Error;

    fn poll_ready_upgradable(&mut self) -> Poll<(), Self::UpgradeError> {
        self.rx.poll_ready()
    }

    fn try_into_upgrade(self, io: Io, read_buf: Bytes) -> Result<Self::Upgrade, (Io, Bytes)> {
        self.rx.upgrade(io, read_buf)
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct AppServiceFuture {
    in_flight: Box<Future<Item = Output, Error = Error> + Send>,
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
            Ok(Async::Ready(out)) => Ok(Async::Ready(self.handle_response(out))),
            Err(err) => self.handle_error(err).map(Into::into),
        }
    }
}

impl AppServiceFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        let in_flight = &mut self.in_flight;
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

    fn handle_response(&mut self, output: Output) -> Response<Body> {
        let (mut response, handler) = output.deconstruct();
        let cx = self.pop_context();

        // TODO: apply middlewares

        #[cfg(feature = "session")]
        cx.cookies.append_to(response.headers_mut());

        if let Some(handler) = handler {
            debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            self.tx.send(handler, cx.request.map(mem::drop));
        }

        response
    }

    fn handle_error(&mut self, err: Error) -> Result<Response<Body>, CritError> {
        let cx = self.pop_context();
        let request = cx.request.map(mem::drop);
        let response = cx.state.error_handler().handle_error(err, &request)?;
        Ok(response.map(ResponseBody::into_hyp))
    }
}
