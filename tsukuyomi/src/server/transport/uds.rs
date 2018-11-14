#![cfg(unix)]

use std::io;
use std::os::unix::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio;
use tokio::net::unix::Incoming;
use tokio::net::{UnixListener, UnixStream};
use tokio::reactor::Handle;

use super::{ConnectionInfo, HasConnectionInfo, Peer, Transport};

impl HasConnectionInfo for UnixStream {
    type Data = Peer<SocketAddr>;
    type Info = UdsConnectionInfo;

    #[inline]
    fn fetch_info(&self) -> io::Result<Self::Info> {
        Ok(UdsConnectionInfo {
            peer_addr: self.peer_addr()?,
        })
    }
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct UdsConnectionInfo {
    peer_addr: SocketAddr,
}

impl ConnectionInfo for UdsConnectionInfo {
    type Data = Peer<SocketAddr>;

    fn data(&self) -> Self::Data {
        Peer(self.peer_addr.clone())
    }
}

impl Transport for PathBuf {
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        (&self).incoming()
    }
}

impl<'a> Transport for &'a PathBuf {
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = tokio::net::unix::Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> std::io::Result<Self::Incoming> {
        <&'a std::path::Path>::incoming(&*self)
    }
}

impl<'a> Transport for &'a Path {
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(UnixListener::bind(self)?.incoming())
    }
}

impl Transport for UnixListener {
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(self.incoming())
    }
}

impl Transport for std::os::unix::net::UnixListener {
    type Io = UnixStream;
    type Error = io::Error;
    type Incoming = Incoming;
    type Data = Peer<SocketAddr>;

    #[inline]
    fn incoming(self) -> io::Result<Self::Incoming> {
        Ok(UnixListener::from_std(self, &Handle::current())?.incoming())
    }
}
