use futures::{future, Future, Poll};
use http::{Request, Response};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use context::Context;
use error::{CritError, Error};
use request::RequestBody;
use response::{Output, ResponseBody};
use router::Router;

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
        future::ok(AppService {
            router: self.app.router.clone(),
        })
    }
}

#[derive(Debug)]
pub struct AppService {
    router: Arc<Router>,
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
            context: cx,
        }
    }
}

pub struct AppServiceFuture {
    in_flight: Box<Future<Item = Output, Error = Error> + Send>,
    context: Context,
}

impl fmt::Debug for AppServiceFuture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AppServiceFuture")
            .field("in_flight", &"<a boxed future>")
            .field("context", &self.context)
            .finish()
    }
}

impl Future for AppServiceFuture {
    type Item = Response<Body>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let in_flight = &mut self.in_flight;
        match self.context.set(|| in_flight.poll()) {
            Ok(out_async) => Ok(out_async.map(|out| out.deconstruct())),
            Err(e) => e.into_response()
                .map(|res| res.map(ResponseBody::into_hyp).into()),
        }
    }
}
