use futures::Stream;
use http::Extensions;
use std::fmt;
use tokio::io::{AsyncRead, AsyncWrite};

use super::imp::CritError;

/// A wrapper type containing a peer address.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peer<T>(T);

impl<T> fmt::Display for Peer<T>
where
    T: fmt::Display,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> std::ops::Deref for Peer<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A trait representing a raw connection to peer.
pub trait Connection: AsyncRead + AsyncWrite {
    type Info: ConnectionInfo;
    type Error: Into<CritError>;

    /// Retrieves the instance of `Self::Info` from this type.
    fn connection_info(&self) -> Result<Self::Info, Self::Error>;
}

pub trait ConnectionInfo {
    fn insert_into(&self, ext: &mut Extensions);
}

/// A trait representing the low-level I/O used in `Server`.
pub trait Transport {
    type Conn: Connection;
    type Error: Into<CritError>;
    type Incoming: Stream<Item = Self::Conn, Error = Self::Error>;

    /// Creates a `Stream` of asynchronous I/Os.
    fn incoming(self) -> Result<Self::Incoming, Self::Error>;
}

mod tcp {
    use super::*;

    use std::io;
    use std::net::SocketAddr;

    use tokio::net::tcp::Incoming;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::reactor::Handle;

    impl Connection for TcpStream {
        type Info = TcpConnectionInfo;
        type Error = io::Error;

        #[inline]
        fn connection_info(&self) -> io::Result<Self::Info> {
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
        fn insert_into(&self, ext: &mut Extensions) {
            ext.insert(Peer(self.peer_addr));
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(use_self))]
    impl Transport for SocketAddr {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            (&self).incoming()
        }
    }

    impl<'a> Transport for &'a SocketAddr {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(TcpListener::bind(self)?.incoming())
        }
    }

    impl Transport for std::net::TcpListener {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            let listener = TcpListener::from_std(self, &Handle::current())?;
            Ok(listener.incoming())
        }
    }

    impl Transport for TcpListener {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }
}

#[cfg(unix)]
mod uds {
    use super::*;

    use std::io;
    use std::os::unix::net::SocketAddr;
    use std::path::{Path, PathBuf};

    use tokio::net::unix::Incoming;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::reactor::Handle;

    impl Connection for UnixStream {
        type Info = UdsConnectionInfo;
        type Error = io::Error;

        #[inline]
        fn connection_info(&self) -> io::Result<Self::Info> {
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
        fn insert_into(&self, ext: &mut Extensions) {
            ext.insert(Peer(self.peer_addr.clone()));
        }
    }

    impl Transport for PathBuf {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            (&self).incoming()
        }
    }

    impl<'a> Transport for &'a PathBuf {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            <&'a std::path::Path>::incoming(&*self)
        }
    }

    impl<'a> Transport for &'a Path {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::bind(self)?.incoming())
        }
    }

    impl Transport for UnixListener {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }

    impl Transport for std::os::unix::net::UnixListener {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::from_std(self, &Handle::current())?.incoming())
        }
    }
}
