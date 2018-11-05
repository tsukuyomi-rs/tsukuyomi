mod tls;
mod uds;

pub use self::imp::Transport;
#[cfg(feature = "tls")]
pub use self::tls::TlsConfig;

// ==== impl ====
mod imp {
    use std;
    use std::io;
    use std::net::SocketAddr;

    use futures::Stream;
    use tokio;
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::server::CritError;

    pub trait Transport: TransportImpl {}

    pub trait TransportImpl {
        type Item: AsyncRead + AsyncWrite + Send + 'static;
        type Error: Into<CritError>;
        type Incoming: Stream<Item = Self::Item, Error = Self::Error> + Send + 'static;

        fn incoming(self) -> io::Result<Self::Incoming>;
    }

    impl Transport for String {}
    impl TransportImpl for String {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            self.as_str().incoming()
        }
    }

    impl<'a> Transport for &'a str {}
    impl<'a> TransportImpl for &'a str {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            self.parse::<std::net::SocketAddr>()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                .incoming()
        }
    }

    impl Transport for SocketAddr {}
    impl TransportImpl for SocketAddr {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            (&self).incoming()
        }
    }

    impl<'a> Transport for &'a SocketAddr {}
    impl<'a> TransportImpl for &'a std::net::SocketAddr {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            Ok(tokio::net::TcpListener::bind(self)?.incoming())
        }
    }

    impl Transport for std::net::TcpListener {}
    impl TransportImpl for std::net::TcpListener {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            let listener =
                tokio::net::TcpListener::from_std(self, &tokio::reactor::Handle::current())?;
            TransportImpl::incoming(listener)
        }
    }

    impl Transport for tokio::net::TcpListener {}
    impl TransportImpl for tokio::net::TcpListener {
        type Item = tokio::net::TcpStream;
        type Error = std::io::Error;
        type Incoming = tokio::net::tcp::Incoming;

        #[inline]
        fn incoming(self) -> std::io::Result<Self::Incoming> {
            Ok(self.incoming())
        }
    }
}
