use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite};

/// A trait representing the low-level I/O used in `Server`.
pub trait Transport {
    type Io: AsyncRead + AsyncWrite;
    type Error;
    type Incoming: Stream<Item = Self::Io, Error = Self::Error>;

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

    #[cfg_attr(feature = "cargo-clippy", allow(use_self))]
    impl Transport for SocketAddr {
        type Io = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            (&self).incoming()
        }
    }

    impl<'a> Transport for &'a SocketAddr {
        type Io = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(TcpListener::bind(self)?.incoming())
        }
    }

    impl Transport for std::net::TcpListener {
        type Io = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            let listener = TcpListener::from_std(self, &Handle::current())?;
            Ok(listener.incoming())
        }
    }

    impl Transport for TcpListener {
        type Io = TcpStream;
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
    use std::path::{Path, PathBuf};

    use tokio::net::unix::Incoming;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::reactor::Handle;

    impl Transport for PathBuf {
        type Io = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            (&self).incoming()
        }
    }

    impl<'a> Transport for &'a PathBuf {
        type Io = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            <&'a std::path::Path>::incoming(&*self)
        }
    }

    impl<'a> Transport for &'a Path {
        type Io = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::bind(self)?.incoming())
        }
    }

    impl Transport for UnixListener {
        type Io = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }

    impl Transport for std::os::unix::net::UnixListener {
        type Io = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn incoming(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::from_std(self, &Handle::current())?.incoming())
        }
    }
}
