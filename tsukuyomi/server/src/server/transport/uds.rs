#![cfg(unix)]

use std::io;
use std::os::unix::net::SocketAddr;
use std::path::{Path, PathBuf};

use http::Extensions;
use tokio;
use tokio::net::unix::Incoming;
use tokio::net::{UnixListener, UnixStream};
use tokio::reactor::Handle;

use super::imp::{ConnectionInfo, HasConnectionInfo, Transport, TransportImpl};

impl HasConnectionInfo for UnixStream {
    type ConnectionInfo = UdsConnectionInfo;

    #[inline]
    fn connection_info(&self) -> Self::ConnectionInfo {
        UdsConnectionInfo {
            peer_addr: self.peer_addr(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct UdsConnectionInfo {
    peer_addr: io::Result<SocketAddr>,
}

impl ConnectionInfo for UdsConnectionInfo {
    fn insert_info(&self, ext: &mut Extensions) {
        if let Ok(ref addr) = self.peer_addr {
            ext.insert(addr.clone());
        }
    }
}

impl Transport for PathBuf {}
impl TransportImpl for PathBuf {
    type Info = UdsConnectionInfo;
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        (&self).incoming()
    }
}

impl<'a> Transport for &'a PathBuf {}
impl<'a> TransportImpl for &'a PathBuf {
    type Info = UdsConnectionInfo;
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = tokio::net::unix::Incoming;

    #[inline]
    fn incoming(self) -> std::io::Result<Self::Incoming> {
        <&'a std::path::Path>::incoming(&*self)
    }
}

impl<'a> Transport for &'a Path {}
impl<'a> TransportImpl for &'a Path {
    type Info = UdsConnectionInfo;
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(UnixListener::bind(self)?.incoming())
    }
}

impl Transport for UnixListener {}
impl TransportImpl for UnixListener {
    type Info = UdsConnectionInfo;
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(self.incoming())
    }
}

impl Transport for std::os::unix::net::UnixListener {}
impl TransportImpl for std::os::unix::net::UnixListener {
    type Info = UdsConnectionInfo;
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(UnixListener::from_std(self, &Handle::current())?.incoming())
    }
}
