use std::sync::Arc;

use futures::{Future, Poll};
use tower_service::NewService;

use super::Middleware;

#[derive(Debug)]
pub struct MiddlewareChain<S, M> {
    new_service: S,
    middleware: Arc<M>,
}

impl<S, M> MiddlewareChain<S, M>
where
    S: NewService,
    M: Middleware<S::Service>,
{
    pub(crate) fn new(new_service: S, middleware: M) -> Self {
        MiddlewareChain {
            new_service,
            middleware: Arc::new(middleware),
        }
    }
}

impl<S, M> NewService for MiddlewareChain<S, M>
where
    S: NewService,
    M: Middleware<S::Service>,
{
    type Request = M::Request;
    type Response = M::Response;
    type Error = M::Error;
    type Service = M::Service;
    type InitError = S::InitError;
    type Future = MiddlewareChainFuture<S::Future, M>;

    fn new_service(&self) -> Self::Future {
        MiddlewareChainFuture {
            future: self.new_service.new_service(),
            middleware: self.middleware.clone(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct MiddlewareChainFuture<F, M> {
    future: F,
    middleware: Arc<M>,
}

impl<F, M> Future for MiddlewareChainFuture<F, M>
where
    F: Future,
    M: Middleware<F::Item>,
{
    type Item = M::Service;
    type Error = F::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future
            .poll()
            .map(|x| x.map(|service| self.middleware.wrap(service)))
    }
}
