use bytes::Bytes;
use futures::future::poll_fn;
use futures::sync::mpsc;
use futures::{future, Async, Future, Poll, Stream};
use http::{Request, Response, StatusCode};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use context::Context;
use error::{CritError, Error};
use input::RequestBody;
use output::{Output, ResponseBody};
use router::Router;
use rt::ServiceExt;
use transport;
use upgrade::UpgradeFn;

use super::App;

pub struct NewAppService {
    pub(super) app: App,
}

impl NewService for NewAppService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Service = AppService;
    type InitError = CritError;
    type Future = future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        let (tx, rx) = mpsc::unbounded();
        future::ok(AppService {
            router: self.app.router.clone(),
            tx: Some(tx),
            rx: rx,
            upgrade: None,
        })
    }
}

#[derive(Debug)]
pub struct AppService {
    router: Arc<Router>,
    tx: Option<mpsc::UnboundedSender<(UpgradeFn, Context)>>,
    rx: mpsc::UnboundedReceiver<(UpgradeFn, Context)>,
    upgrade: Option<(UpgradeFn, Context)>,
}

impl Service for AppService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = CritError;
    type Future = AppServiceFuture;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        let (parts, payload) = request.into_parts();
        let mut cx = Context {
            request: Request::from_parts(parts, ()),
            payload: RefCell::new(Some(RequestBody::from_hyp(payload))),
        };
        let in_flight = self.router.handle(&mut cx);
        AppServiceFuture {
            in_flight: in_flight,
            context: Some(cx),
            tx: self.tx.as_ref().unwrap().clone(),
        }
    }
}

impl ServiceExt<transport::Io> for AppService {
    type Upgrade = Box<Future<Item = (), Error = ()> + Send>;
    type UpgradeError = ::failure::Error;

    fn poll_ready_upgrade(&mut self) -> Poll<(), Self::UpgradeError> {
        let _ = self.tx.take();

        match try_ready!(self.rx.poll().map_err(|_| format_err!("during rx.poll()"))) {
            Some(upgrade) => {
                self.upgrade = Some(upgrade);
                Ok(().into())
            }
            None => Err(format_err!("rx is empty")),
        }
    }

    fn upgrade(mut self, io: transport::Io, read_buf: Bytes) -> Self::Upgrade {
        trace!("AppService::upgrade");

        debug_assert!(self.upgrade.is_some());
        let (mut upgrade, cx) = self.upgrade.take().unwrap();

        let mut upgraded = upgrade.upgrade(io, read_buf, &cx);

        Box::new(poll_fn(move || cx.set(|| upgraded.poll())))
    }
}

pub struct AppServiceFuture {
    in_flight: Box<Future<Item = Output, Error = Error> + Send>,
    context: Option<Context>,
    tx: mpsc::UnboundedSender<(UpgradeFn, Context)>,
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
                let (response, upgrade) = out.deconstruct();
                if let Some(upgrade) = upgrade {
                    debug_assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
                    let cx = self.context
                        .take()
                        .expect("AppServiceFuture has already resolved/rejected");
                    let _ = self.tx.unbounded_send((upgrade, cx));
                }
                Ok(Async::Ready(response))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => e.into_response()
                .map(|res| res.map(ResponseBody::into_hyp).into()),
        }
    }
}
