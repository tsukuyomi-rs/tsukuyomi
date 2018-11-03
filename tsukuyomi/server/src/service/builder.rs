use super::middleware::{Middleware, MiddlewareChain};
use tower_service::NewService;

pub trait NewServiceExt: NewService {
    fn with_middleware<M>(self, middleware: M) -> MiddlewareChain<Self, M>
    where
        Self: Sized,
        M: Middleware<Self::Service>,
    {
        MiddlewareChain::new(self, middleware)
    }
}

impl<S: NewService> NewServiceExt for S {}
