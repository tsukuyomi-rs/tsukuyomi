use std::io;
use std::net::SocketAddr;

use http::Extensions;
use tokio::net::tcp::Incoming;
use tokio::net::{TcpListener, TcpStream};
use tokio::reactor::Handle;

use super::imp::{ConnectionInfo, HasConnectionInfo, Transport, TransportImpl};

impl HasConnectionInfo for TcpStream {
    type ConnectionInfo = TcpConnectionInfo;

    #[inline]
    fn connection_info(&self) -> Self::ConnectionInfo {
        TcpConnectionInfo {
            peer_addr: self.peer_addr(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct TcpConnectionInfo {
    peer_addr: io::Result<SocketAddr>,
}

impl ConnectionInfo for TcpConnectionInfo {
    fn insert_info(&self, ext: &mut Extensions) {
        if let Ok(addr) = self.peer_addr {
            ext.insert(addr);
        }
    }
}

impl Transport for SocketAddr {}
impl TransportImpl for SocketAddr {
    type Info = TcpConnectionInfo;
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        (&self).incoming()
    }
}

impl<'a> Transport for &'a SocketAddr {}
impl<'a> TransportImpl for &'a SocketAddr {
    type Info = TcpConnectionInfo;
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(TcpListener::bind(self)?.incoming())
    }
}

impl Transport for std::net::TcpListener {}
impl TransportImpl for std::net::TcpListener {
    type Info = TcpConnectionInfo;
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        let listener = TcpListener::from_std(self, &Handle::current())?;
        TransportImpl::incoming(listener)
    }
}

impl Transport for TcpListener {}
impl TransportImpl for TcpListener {
    type Info = TcpConnectionInfo;
    type Io = TcpStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(self.incoming())
    }
}
