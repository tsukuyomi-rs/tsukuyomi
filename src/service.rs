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
use response::ResponseBody;
use router::Router;

#[derive(Debug)]
pub struct MyService {
    router: Arc<Router>,
}

impl Service for MyService {
    type ReqBody = Body;
    type ResBody = ResponseBody;
    type Error = CritError;
    type Future = MyServiceFuture;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        let (parts, payload) = request.into_parts();
        let mut cx = Context {
            request: Request::from_parts(parts, ()),
            payload: RefCell::new(Some(RequestBody::from_hyp(payload))),
        };
        let in_flight = self.router.handle(&mut cx);
        MyServiceFuture {
            in_flight: in_flight,
            context: cx,
        }
    }
}

pub struct MyServiceFuture {
    in_flight: Box<Future<Item = Response<ResponseBody>, Error = Error> + Send>,
    context: Context,
}

impl fmt::Debug for MyServiceFuture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MyServiceFuture")
            .field("in_flight", &"<a boxed future>")
            .field("context", &self.context)
            .finish()
    }
}

impl Future for MyServiceFuture {
    type Item = Response<ResponseBody>;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let in_flight = &mut self.in_flight;
        match self.context.set(|| in_flight.poll()) {
            Ok(x) => Ok(x),
            Err(e) => e.into_response().map(Into::into),
        }
    }
}

#[derive(Debug)]
pub struct NewMyService {
    router: Arc<Router>,
}

impl NewMyService {
    pub fn new(router: Router) -> NewMyService {
        NewMyService {
            router: Arc::new(router),
        }
    }
}

impl NewService for NewMyService {
    type ReqBody = Body;
    type ResBody = ResponseBody;
    type Error = CritError;
    type Service = MyService;
    type InitError = CritError;
    type Future = future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        future::ok(MyService {
            router: self.router.clone(),
        })
    }
}
