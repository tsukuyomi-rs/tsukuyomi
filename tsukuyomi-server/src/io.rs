use {
    crate::CritError,
    futures::{Future, IntoFuture, Stream},
    tokio::io::{AsyncRead, AsyncWrite},
};

/// A trait that represents the low-level I/O.
pub trait Listener {
    type Conn: AsyncRead + AsyncWrite;
    type Error: Into<CritError>;
    type Incoming: Stream<Item = Self::Conn, Error = Self::Error>;

    /// Creates a `Stream` of asynchronous I/Os.
    fn listen(self) -> Result<Self::Incoming, Self::Error>;
}

/// A trait that represents the conversion of asynchronous I/Os.
///
/// Typically, the implementors of this trait establish a TLS session.
pub trait Acceptor<T> {
    type Conn: AsyncRead + AsyncWrite;
    type Error;
    type Accept: Future<Item = Self::Conn, Error = Self::Error>;

    fn accept(&self, io: T) -> Self::Accept;
}

impl<F, T, R> Acceptor<T> for F
where
    F: Fn(T) -> R,
    R: IntoFuture,
    R::Item: AsyncRead + AsyncWrite,
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
    T: AsyncRead + AsyncWrite,
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
        super::Listener,
        std::{io, net::SocketAddr},
        tokio::{
            net::{tcp::Incoming, TcpListener, TcpStream},
            reactor::Handle,
        },
    };

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
        super::Listener,
        std::{
            io,
            path::{Path, PathBuf},
        },
        tokio::{
            net::{unix::Incoming, UnixListener, UnixStream},
            reactor::Handle,
        },
    };

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
        super::Acceptor,
        tokio::io::{AsyncRead, AsyncWrite},
        tokio_tls::{Accept, TlsAcceptor, TlsStream},
    };

    impl<T> Acceptor<T> for TlsAcceptor
    where
        T: AsyncRead + AsyncWrite,
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
        super::Acceptor,
        rustls::ServerSession,
        tokio::io::{AsyncRead, AsyncWrite},
        tokio_rustls::{Accept, TlsAcceptor, TlsStream},
    };

    impl<T> Acceptor<T> for TlsAcceptor
    where
        T: AsyncRead + AsyncWrite,
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
        super::Acceptor,
        openssl::ssl::{HandshakeError, SslAcceptor},
        tokio::io::{AsyncRead, AsyncWrite},
        tokio_openssl::{AcceptAsync, SslAcceptorExt, SslStream},
    };

    impl<T> Acceptor<T> for SslAcceptor
    where
        T: AsyncRead + AsyncWrite,
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
