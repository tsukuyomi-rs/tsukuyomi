use futures::{future, Future, Poll};
use http::{Request, Response};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::cell::RefCell;
use std::sync::Arc;

use context::Context;
use error::{CritError, Error};
use handler::Handler;
use request::RequestBody;
use response::ResponseBody;

#[derive(Debug)]
pub struct MyService<H: Handler> {
    handler: Arc<H>,
}

impl<H: Handler> Service for MyService<H> {
    type ReqBody = Body;
    type ResBody = ResponseBody;
    type Error = CritError;
    type Future = MyServiceFuture<H::Future>;

    fn call(&mut self, request: Request<Self::ReqBody>) -> Self::Future {
        let (parts, payload) = request.into_parts();
        let cx = Context {
            request: Request::from_parts(parts, ()),
            payload: RefCell::new(Some(RequestBody::from_hyp(payload))),
        };
        let in_flight = self.handler.handle_async(&cx);
        MyServiceFuture {
            in_flight: in_flight,
            context: cx,
        }
    }
}

#[derive(Debug)]
pub struct MyServiceFuture<T> {
    in_flight: T,
    context: Context,
}

impl<T> Future for MyServiceFuture<T>
where
    T: Future<Item = Response<ResponseBody>, Error = Error>,
{
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
pub struct NewMyService<H: Handler> {
    handler: Arc<H>,
}

impl<H: Handler> NewService for NewMyService<H> {
    type ReqBody = Body;
    type ResBody = ResponseBody;
    type Error = CritError;
    type Service = MyService<H>;
    type InitError = CritError;
    type Future = future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        future::ok(MyService {
            handler: self.handler.clone(),
        })
    }
}

pub fn new_service<H: Handler>(handler: H) -> NewMyService<H> {
    NewMyService {
        handler: Arc::new(handler),
    }
}
