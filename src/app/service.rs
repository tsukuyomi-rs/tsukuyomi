use bytes::Bytes;
use futures::{future, Async, Future, Poll};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::fmt;
use std::sync::Arc;

use context::Context;
use error::{CritError, Error};
use input::RequestBody;
use output::{Output, ResponseBody};
use router::{Router, RouterState};
use server::{Io, ServiceUpgradeExt};
use upgrade::service as upgrade;

use super::App;

impl App {
    pub fn new_service(&self) -> AppService {
        AppService {
            router: self.router.clone(),
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
    router: Arc<Router>,
    rx: upgrade::Receiver,
}

impl Service for AppService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Future = AppServiceFuture;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        let mut cx = Context {
            request: request.map(RequestBody::from_hyp),
            route: RouterState::Uninitialized,
            router: self.router.clone(),
        };
        let in_flight = self.router.handle(&mut cx);
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
        let in_flight = &mut self.in_flight;
        match {
            let cx = self.context
                .as_ref()
                .expect("AppServiceFuture has already resolved/rejected");
            cx.set(|| in_flight.poll())
        } {
            Ok(Async::Ready(out)) => {
                let (response, handler) = out.deconstruct();
                if let Some(handler) = handler {
                    debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
                    let cx = self.context
                        .take()
                        .expect("AppServiceFuture has already resolved/rejected");
                    self.tx.send(handler, cx.request.map(|bd| drop(bd)));
                }
                Ok(Async::Ready(response))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => e.into_response().map(|res| res.map(ResponseBody::into_hyp).into()),
        }
    }
}
