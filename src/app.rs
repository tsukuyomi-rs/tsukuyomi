use futures::{future, Future, Poll};
use http::{Request, Response};
use hyper::body::Body;
use hyper::service::{NewService, Service};
use std::cell::RefCell;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use context::Context;
use error::{CritError, Error};
use request::RequestBody;
use response::ResponseBody;
use router::Router;
use rt;

#[derive(Debug)]
pub struct App {
    router: Arc<Router>,
    addr: SocketAddr,
}

impl App {
    pub fn new(router: Router) -> App {
        App {
            router: Arc::new(router),
            addr: ([127, 0, 0, 1], 4000).into(),
        }
    }

    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    pub fn serve(self) -> rt::Result<()> {
        let addr = self.addr;
        rt::serve(self, &addr)
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
        future::ok(AppService {
            router: self.router.clone(),
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
    in_flight: Box<Future<Item = Response<ResponseBody>, Error = Error> + Send>,
    context: Context,
}

impl fmt::Debug for AppServiceFuture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MyServiceFuture")
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
            Ok(x) => Ok(x),
            Err(e) => e.into_response().map(Into::into),
        }
    }
}
