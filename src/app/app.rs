use futures::future;
use hyper::body::Body;
use hyper::service::NewService;
use std::net::SocketAddr;
use std::sync::Arc;

use super::service::AppService;
use error::CritError;
use router::{self, Route, Router};
use rt;

#[derive(Debug)]
pub struct App {
    router: Arc<Router>,
    addr: SocketAddr,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder {
            router: Router::builder(),
        }
    }

    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    pub fn new_service(&self) -> AppService {
        AppService {
            router: self.router.clone(),
        }
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
        future::ok(self.new_service())
    }
}

#[derive(Debug)]
pub struct AppBuilder {
    router: router::Builder,
}

impl AppBuilder {
    pub fn mount<I>(&mut self, routes: I) -> &mut Self
    where
        I: IntoIterator<Item = Route>,
    {
        self.router.mount(routes);
        self
    }

    pub fn finish(&mut self) -> rt::Result<App> {
        Ok(App {
            router: self.router.finish().map(Arc::new)?,
            addr: ([127, 0, 0, 1], 4000).into(),
        })
    }
}
