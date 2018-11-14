use std::io;
use std::net::SocketAddr;

use tokio::net::tcp::Incoming;
use tokio::net::{TcpListener, TcpStream};
use tokio::reactor::Handle;

use super::{ConnectionInfo, HasConnectionInfo, Peer, Transport};

impl HasConnectionInfo for TcpStream {
    type Data = Peer<SocketAddr>;
    type Info = TcpConnectionInfo;

    #[inline]
    fn fetch_info(&self) -> io::Result<Self::Info> {
        Ok(TcpConnectionInfo {
            peer_addr: self.peer_addr()?,
        })
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct TcpConnectionInfo {
    peer_addr: SocketAddr,
}

impl ConnectionInfo for TcpConnectionInfo {
    type Data = Peer<SocketAddr>;

    fn data(&self) -> Self::Data {
        Peer(self.peer_addr)
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl Transport for SocketAddr {
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        (&self).incoming()
    }
}

impl<'a> Transport for &'a SocketAddr {
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(TcpListener::bind(self)?.incoming())
    }
}

impl Transport for std::net::TcpListener {
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        let listener = TcpListener::from_std(self, &Handle::current())?;
        Transport::incoming(listener)
    }
}

impl Transport for TcpListener {
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(self.incoming())
    }
}
