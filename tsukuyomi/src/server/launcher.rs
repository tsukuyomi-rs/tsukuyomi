use super::acceptor::Acceptor;
use super::connection_info::{ConnectionInfo, HasConnectionInfo};
use super::http::{HttpRequest, HttpResponse};
use super::imp::CritError;
use super::middleware::Middleware;
use super::transport::Transport;
use futures::{Future, Poll, Stream};
use http::{Request, Response};
use hyper::body::{Body, Payload};
use hyper::server::conn::Http;
use std::rc::Rc;
use std::sync::Arc;
use tower_service::{NewService, Service};

macro_rules! serve {
    (
        new_service: $new_service:expr,
        middleware: $middleware:expr,
        transport: $transport:expr,
        acceptor: $acceptor:expr,
        protocol: $protocol:expr,
        spawn: $spawn:expr,
    ) => {{
        let new_service = $new_service;
        let middleware = $middleware;
        let transport = $transport;
        let acceptor = $acceptor;
        let protocol = $protocol;
        let spawn = $spawn;

        transport
            .incoming()
            .map_err(|err| failure::Error::from_boxed_compat(err.into()))?
            .map_err(|_e| log::error!("transport error"))
            .for_each(move |io| {
                let info = io.fetch_info();
                if let Err(..) = info {
                    log::error!("failed to fetch the connection information.");
                }
                let info = info.ok();

                let accept = acceptor
                    .accept(io)
                    .map_err(|_e| log::error!("acceptor error"));

                let middleware = middleware.clone();
                let protocol = protocol.clone();
                let service = new_service
                    .new_service()
                    .map_err(|_e| log::error!("new_service error"))
                    .map(move |service| middleware.wrap(service));

                let task = accept.and_then(move |io| {
                    service
                        .map(move |service| LiftedHttpService { service, info })
                        .and_then(move |service| {
                            protocol
                                .serve_connection(io, service)
                                .with_upgrades()
                                .map_err(|e| log::error!("HTTP protocol error: {}", e))
                        })
                });
                spawn(task);
                Ok(())
            })
    }};
}

#[allow(missing_debug_implementations)]
struct LiftedHttpService<S, T> {
    service: S,
    info: Option<T>,
}

impl<S, T> hyper::service::Service for LiftedHttpService<S, T>
where
    S: Service,
    S::Request: HttpRequest,
    S::Response: HttpResponse,
    <S::Request as HttpRequest>::Body: From<Body>,
    <S::Response as HttpResponse>::Body: Payload,
    S::Error: Into<CritError>,
    T: ConnectionInfo,
    T::Data: Send + Sync + 'static,
{
    type ReqBody = Body;
    type ResBody = <S::Response as HttpResponse>::Body;
    type Error = S::Error;
    type Future = LiftedHttpServiceFuture<S::Future>;

    #[inline]
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if let Some(ref info) = self.info {
            request.extensions_mut().insert(info.data());
        }
        let request =
            S::Request::from_request(request.map(<S::Request as HttpRequest>::Body::from));
        LiftedHttpServiceFuture(self.service.call(request))
    }
}

#[allow(missing_debug_implementations)]
struct LiftedHttpServiceFuture<F>(F);

impl<F> Future for LiftedHttpServiceFuture<F>
where
    F: Future,
    F::Item: HttpResponse,
    F::Error: Into<CritError>,
{
    type Item = Response<<F::Item as HttpResponse>::Body>;
    type Error = F::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0
            .poll()
            .map(|x| x.map(|response| response.into_response()))
    }
}

pub trait Launcher<S, M, T, A> {
    fn launch(
        self,
        new_service: S,
        middleware: M,
        transport: T,
        acceptor: A,
        protocol: Http,
    ) -> super::Result<()>;
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct DefaultLauncher(());

impl<S, M, T, A> Launcher<S, M, T, A> for DefaultLauncher
where
    S: NewService + Send + 'static,
    S::InitError: Into<CritError>,
    S::Future: Send + 'static,
    M: Middleware<S::Service> + Send + Sync + 'static,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    M::Service: Send + 'static,
    <M::Service as Service>::Future: Send + 'static,
    T: Transport,
    T::Io: HasConnectionInfo + Send + 'static,
    <T::Io as HasConnectionInfo>::Info: Send + 'static,
    <T::Io as HasConnectionInfo>::Data: Send + Sync + 'static,
    T::Error: Into<CritError>,
    T::Incoming: Send + 'static,
    A: Acceptor<T::Io> + Send + 'static,
    A::Accepted: Send + 'static,
    A::Future: Send + 'static,
{
    fn launch(
        self,
        new_service: S,
        middleware: M,
        transport: T,
        acceptor: A,
        protocol: Http,
    ) -> super::Result<()> {
        let runtime = tokio::runtime::Runtime::new()?;
        launch_default(
            runtime,
            new_service,
            middleware,
            transport,
            acceptor,
            protocol,
        )
    }
}

impl<S, M, T, A> Launcher<S, M, T, A> for tokio::runtime::Runtime
where
    S: NewService + Send + 'static,
    S::InitError: Into<CritError>,
    S::Future: Send + 'static,
    M: Middleware<S::Service> + Send + Sync + 'static,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    M::Service: Send + 'static,
    <M::Service as Service>::Future: Send + 'static,
    T: Transport,
    T::Io: HasConnectionInfo + Send + 'static,
    <T::Io as HasConnectionInfo>::Info: Send + 'static,
    <T::Io as HasConnectionInfo>::Data: Send + Sync + 'static,
    T::Error: Into<CritError>,
    T::Incoming: Send + 'static,
    A: Acceptor<T::Io> + Send + 'static,
    A::Accepted: Send + 'static,
    A::Future: Send + 'static,
{
    fn launch(
        self,
        new_service: S,
        middleware: M,
        transport: T,
        acceptor: A,
        protocol: Http,
    ) -> super::Result<()> {
        launch_default(self, new_service, middleware, transport, acceptor, protocol)
    }
}

fn launch_default<S, M, T, A>(
    runtime: tokio::runtime::Runtime,
    new_service: S,
    middleware: M,
    transport: T,
    acceptor: A,
    protocol: Http,
) -> super::Result<()>
where
    S: NewService + Send + 'static,
    S::InitError: Into<CritError>,
    S::Future: Send + 'static,
    M: Middleware<S::Service> + Send + Sync + 'static,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    M::Service: Send + 'static,
    <M::Service as Service>::Future: Send + 'static,
    T: Transport,
    T::Io: HasConnectionInfo + Send + 'static,
    <T::Io as HasConnectionInfo>::Info: Send + 'static,
    <T::Io as HasConnectionInfo>::Data: Send + Sync + 'static,
    T::Error: Into<CritError>,
    T::Incoming: Send + 'static,
    A: Acceptor<T::Io> + Send + 'static,
    A::Accepted: Send + 'static,
    A::Future: Send + 'static,
{
    let serve = serve!{
        new_service: new_service,
        middleware: Arc::new(middleware),
        transport: transport,
        acceptor: acceptor,
        protocol: Arc::new(
            protocol.with_executor(tokio::executor::DefaultExecutor::current()),
        ),
        spawn: |fut| crate::rt::spawn(fut),
    };

    let _ = runtime.block_on_all(serve);
    Ok(())
}

#[derive(Debug, Default)]
pub struct CurrentThread(());

impl<S, M, T, A> Launcher<S, M, T, A> for CurrentThread
where
    S: NewService + 'static,
    S::InitError: Into<CritError>,
    S::Future: 'static,
    M: Middleware<S::Service> + 'static,
    M::Service: 'static,
    <M::Service as Service>::Future: 'static,
    M::Request: HttpRequest,
    M::Response: HttpResponse,
    <M::Request as HttpRequest>::Body: From<Body>,
    <M::Response as HttpResponse>::Body: Payload,
    M::Error: Into<CritError>,
    T: Transport,
    T::Io: HasConnectionInfo + 'static,
    <T::Io as HasConnectionInfo>::Data: Send + Sync + 'static,
    <T::Io as HasConnectionInfo>::Info: 'static,
    T::Error: Into<CritError>,
    T::Incoming: 'static,
    A: Acceptor<T::Io> + 'static,
    A::Accepted: Send + 'static,
    A::Future: 'static,
{
    fn launch(
        self,
        new_service: S,
        middleware: M,
        transport: T,
        acceptor: A,
        protocol: Http,
    ) -> super::Result<()> {
        use tokio::runtime::current_thread as rt;

        let serve = serve!{
            new_service: new_service,
            middleware: Rc::new(middleware),
            transport: transport,
            acceptor: acceptor,
            protocol: Rc::new(protocol.with_executor(rt::TaskExecutor::current())),
            spawn: |fut| rt::spawn(fut),
        };

        let _ = rt::block_on_all(serve);
        Ok(())
    }
}
