use {
    super::CritError,
    futures::{Future, IntoFuture, Stream},
    http::Extensions,
    std::fmt,
    tokio::io::{AsyncRead, AsyncWrite},
};

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

/// A trait representing the low-level I/O.
pub trait Listener {
    type Conn: Connection;
    type Error: Into<CritError>;
    type Incoming: Stream<Item = Self::Conn, Error = Self::Error>;

    /// Creates a `Stream` of asynchronous I/Os.
    fn listen(self) -> Result<Self::Incoming, Self::Error>;
}

pub trait Acceptor<T> {
    type Conn: Connection;
    type Error;
    type Accept: Future<Item = Self::Conn, Error = Self::Error>;

    fn accept(&self, io: T) -> Self::Accept;
}

impl<F, T, R> Acceptor<T> for F
where
    F: Fn(T) -> R,
    R: IntoFuture,
    R::Item: Connection,
{
    type Conn = R::Item;
    type Error = R::Error;
    type Accept = R::Future;

    #[inline]
    fn accept(&self, io: T) -> Self::Accept {
        (*self)(io).into_future()
    }
}

impl<T> Acceptor<T> for ()
where
    T: Connection,
{
    type Conn = T;
    type Error = std::io::Error;
    type Accept = futures::future::FutureResult<Self::Conn, Self::Error>;

    #[inline]
    fn accept(&self, io: T) -> Self::Accept {
        futures::future::ok(io)
    }
}

mod tcp {
    use {
        super::{Connection, ConnectionInfo, Listener, Peer},
        http::Extensions,
        std::{io, net::SocketAddr},
        tokio::{
            net::{tcp::Incoming, TcpListener, TcpStream},
            reactor::Handle,
        },
    };

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
    impl Listener for SocketAddr {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            (&self).listen()
        }
    }

    impl<'a> Listener for &'a SocketAddr {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            Ok(TcpListener::bind(self)?.incoming())
        }
    }

    impl Listener for std::net::TcpListener {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            let listener = TcpListener::from_std(self, &Handle::current())?;
            Ok(listener.incoming())
        }
    }

    impl Listener for TcpListener {
        type Conn = TcpStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }
}

#[cfg(unix)]
mod uds {
    use {
        super::{Connection, ConnectionInfo, Listener, Peer},
        http::Extensions,
        std::{
            io,
            os::unix::net::SocketAddr,
            path::{Path, PathBuf},
        },
        tokio::{
            net::{unix::Incoming, UnixListener, UnixStream},
            reactor::Handle,
        },
    };

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

    impl Listener for PathBuf {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            (&self).listen()
        }
    }

    impl<'a> Listener for &'a PathBuf {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            <&'a std::path::Path>::listen(&*self)
        }
    }

    impl<'a> Listener for &'a Path {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::bind(self)?.incoming())
        }
    }

    impl Listener for UnixListener {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }

    impl Listener for std::os::unix::net::UnixListener {
        type Conn = UnixStream;
        type Error = io::Error;
        type Incoming = Incoming;

        #[inline]
        fn listen(self) -> io::Result<Self::Incoming> {
            Ok(UnixListener::from_std(self, &Handle::current())?.incoming())
        }
    }
}

#[cfg(feature = "use-native-tls")]
mod navite_tls {
    use {
        super::{Acceptor, Connection},
        tokio_tls::{Accept, TlsAcceptor, TlsStream},
    };

    impl<T> Connection for TlsStream<T>
    where
        T: Connection,
    {
        type Info = T::Info;
        type Error = T::Error;

        #[inline]
        fn connection_info(&self) -> Result<Self::Info, Self::Error> {
            self.get_ref() // <-- tokio_tls::TlsStream
                .get_ref() // <-- native_tls::TlsStream
                .connection_info()
        }
    }

    impl<T> Acceptor<T> for TlsAcceptor
    where
        T: Connection,
    {
        type Conn = TlsStream<T>;
        type Error = native_tls::Error;
        type Accept = Accept<T>;

        #[inline]
        fn accept(&self, io: T) -> Self::Accept {
            self.accept(io)
        }
    }
}

#[cfg(feature = "use-rustls")]
mod rustls {
    use {
        super::{Acceptor, Connection},
        rustls::ServerSession,
        tokio_rustls::{Accept, TlsAcceptor, TlsStream},
    };

    impl<T> Connection for TlsStream<T, ServerSession>
    where
        T: Connection,
    {
        type Info = T::Info;
        type Error = T::Error;

        #[inline]
        fn connection_info(&self) -> Result<Self::Info, Self::Error> {
            self.get_ref().0.connection_info()
        }
    }

    impl<T> Acceptor<T> for TlsAcceptor
    where
        T: Connection,
    {
        type Conn = TlsStream<T, ServerSession>;
        type Error = std::io::Error;
        type Accept = Accept<T>;

        #[inline]
        fn accept(&self, io: T) -> Self::Accept {
            self.accept(io)
        }
    }
}

#[cfg(feature = "use-openssl")]
mod openssl {
    use {
        super::{Acceptor, Connection},
        openssl::ssl::{HandshakeError, SslAcceptor},
        tokio_openssl::{AcceptAsync, SslAcceptorExt, SslStream},
    };

    impl<T> Connection for SslStream<T>
    where
        T: Connection,
    {
        type Info = T::Info;
        type Error = T::Error;

        #[inline]
        fn connection_info(&self) -> Result<Self::Info, Self::Error> {
            self.get_ref() // <-- tokio_openssl::SslStream
                .get_ref() // <-- openssl::ssl::SslStream
                .connection_info()
        }
    }

    impl<T> Acceptor<T> for SslAcceptor
    where
        T: Connection,
    {
        type Conn = SslStream<T>;
        type Error = HandshakeError<T>;
        type Accept = AcceptAsync<T>;

        #[inline]
        fn accept(&self, io: T) -> Self::Accept {
            self.accept_async(io)
        }
    }
}
