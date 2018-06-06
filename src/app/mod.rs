pub mod service;

use std::net::SocketAddr;
use std::sync::Arc;

use router::{self, Route, Router};
use rt;

use self::service::NewAppService;

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

    pub fn serve(self) -> rt::Result<()> {
        let addr = self.addr;
        rt::serve(self.lift_new_service(), &addr)
    }

    fn lift_new_service(self) -> NewAppService {
        NewAppService { app: self }
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
