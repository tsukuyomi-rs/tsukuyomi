use futures::Future;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Acceptor<Io: AsyncRead + AsyncWrite> {
    type Accepted: AsyncRead + AsyncWrite;
    type Error;
    type Future: Future<Item = Self::Accepted, Error = Self::Error>;

    fn accept(&self, io: Io) -> Self::Future;
}

#[derive(Debug, Clone, Default)]
pub struct Raw(());

impl<Io> Acceptor<Io> for Raw
where
    Io: AsyncRead + AsyncWrite,
{
    type Accepted = Io;
    type Error = std::io::Error;
    type Future = futures::future::FutureResult<Self::Accepted, Self::Error>;

    #[inline]
    fn accept(&self, io: Io) -> Self::Future {
        futures::future::ok(io)
    }
}

#[cfg(feature = "use-native-tls")]
mod navite_tls {
    use super::Acceptor;

    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_tls::{Accept, TlsAcceptor, TlsStream};

    impl<Io> Acceptor<Io> for TlsAcceptor
    where
        Io: AsyncRead + AsyncWrite,
    {
        type Accepted = TlsStream<Io>;
        type Error = native_tls::Error;
        type Future = Accept<Io>;

        #[inline]
        fn accept(&self, io: Io) -> Self::Future {
            self.accept(io)
        }
    }
}

#[cfg(feature = "use-rustls")]
mod rustls {
    use super::Acceptor;

    use rustls::ServerSession;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_rustls::{Accept, TlsAcceptor, TlsStream};

    impl<Io> Acceptor<Io> for TlsAcceptor
    where
        Io: AsyncRead + AsyncWrite,
    {
        type Accepted = TlsStream<Io, ServerSession>;
        type Error = std::io::Error;
        type Future = Accept<Io>;

        #[inline]
        fn accept(&self, io: Io) -> Self::Future {
            self.accept(io)
        }
    }
}

#[cfg(feature = "use-openssl")]
mod openssl {
    use super::Acceptor;

    use openssl::ssl::{HandshakeError, SslAcceptor};
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio_openssl::{AcceptAsync, SslAcceptorExt, SslStream};

    impl<Io> Acceptor<Io> for SslAcceptor
    where
        Io: AsyncRead + AsyncWrite,
    {
        type Accepted = SslStream<Io>;
        type Error = HandshakeError<Io>;
        type Future = AcceptAsync<Io>;

        #[inline]
        fn accept(&self, io: Io) -> Self::Future {
            self.accept_async(io)
        }
    }
}