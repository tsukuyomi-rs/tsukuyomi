use {
    futures::Poll,
    http::Request,
    std::{fmt, io, net::SocketAddr},
    tokio::net::TcpStream,
    tsukuyomi::input::body::RequestBody,
    tsukuyomi_service::{modify_service_ref, ModifyService, Service},
};

#[derive(Debug, Clone)]
pub struct PeerAddr(SocketAddr);

impl fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Creates a `ModifyService` which inserts the peer's address into the extension map of `Request`
/// before calling the internal service.
///
/// If an error occurs when acquiring the peer address, the construction of service will fail.
pub fn with_peer_addr<S, Bd>() -> impl for<'a> ModifyService<
    &'a TcpStream, //
    Request<Bd>,
    S,
    Response = S::Response,
    Error = S::Error,
    Service = WithPeerAddr<S>,
    ModifyError = io::Error,
    Future = futures::future::FutureResult<WithPeerAddr<S>, io::Error>,
>
where
    S: Service<Request<Bd>>,
    RequestBody: From<Bd>,
{
    modify_service_ref(|service: S, io: &TcpStream| {
        Ok(WithPeerAddr {
            service,
            peer_addr: io.peer_addr().map(PeerAddr)?,
        })
    })
}

#[allow(missing_debug_implementations)]
pub struct WithPeerAddr<S> {
    service: S,
    peer_addr: PeerAddr,
}

impl<S, Bd> Service<Request<Bd>> for WithPeerAddr<S>
where
    S: Service<Request<Bd>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    #[inline]
    fn call(&mut self, mut request: Request<Bd>) -> Self::Future {
        request.extensions_mut().insert(self.peer_addr.clone());
        self.service.call(request)
    }
}
