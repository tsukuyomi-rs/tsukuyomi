use {
    super::transport::Connection,
    futures::{Future, IntoFuture},
};

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

#[derive(Debug, Clone)]
pub struct Raw(());

impl Raw {
    pub(crate) fn new() -> Self {
        Raw(())
    }
}

impl<T> Acceptor<T> for Raw
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
