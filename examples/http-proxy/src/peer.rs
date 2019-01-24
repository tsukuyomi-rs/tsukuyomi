use {
    futures::Poll,
    http::Request,
    izanami_service::Service,
    std::{fmt, io, net::SocketAddr},
    tokio::net::TcpStream,
    tsukuyomi::app::ModifyService,
    tsukuyomi::input::body::RequestBody,
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
pub fn with_peer_addr() -> WithPeerAddr {
    WithPeerAddr(())
}

#[derive(Debug)]
pub struct WithPeerAddr(());

impl<'a, S, Bd> ModifyService<&'a TcpStream, Request<Bd>, S> for WithPeerAddr
where
    S: Service<Request<Bd>>,
    RequestBody: From<Bd>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = WithPeerAddrService<S>;
    type ModifyError = io::Error;
    type Future = futures::future::FutureResult<Self::Service, Self::ModifyError>;

    fn modify_service(&self, service: S, io: &'a TcpStream) -> Self::Future {
        futures::future::result(
            io.peer_addr()
                .map(PeerAddr)
                .map(|peer_addr| WithPeerAddrService { service, peer_addr }),
        )
    }
}

#[allow(missing_debug_implementations)]
pub struct WithPeerAddrService<S> {
    service: S,
    peer_addr: PeerAddr,
}

impl<S, Bd> Service<Request<Bd>> for WithPeerAddrService<S>
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
