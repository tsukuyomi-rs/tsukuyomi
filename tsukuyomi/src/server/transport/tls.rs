#![cfg(feature = "tls")]

use std::io;

use futures::{Future, Stream};
use rustls::ServerSession;
use tokio_rustls::{TlsAcceptor, TlsStream};

use super::{HasConnectionInfo, Transport};
use crate::server::CritError;

pub fn tls<T, A>(raw_transport: T, acceptor: A) -> TlsConfig<T>
where
    T: Transport,
    T::Io: Send + 'static,
    T::Error: Into<CritError> + 'static,
    T::Incoming: Send + 'static,
    A: Into<TlsAcceptor>,
{
    TlsConfig::new(raw_transport, acceptor)
}

#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct TlsConfig<T> {
    raw_transport: T,
    acceptor: TlsAcceptor,
}

impl<T> TlsConfig<T>
where
    T: Transport,
    T::Io: Send + 'static,
    T::Error: Into<CritError> + 'static,
    T::Incoming: Send + 'static,
{
    pub fn new<A>(raw_transport: T, acceptor: A) -> Self
    where
        A: Into<TlsAcceptor>,
    {
        Self {
            raw_transport,
            acceptor: acceptor.into(),
        }
    }
}

impl<T> Transport for TlsConfig<T>
where
    T: Transport,
    T::Io: Send + 'static,
    T::Error: Into<CritError> + 'static,
    T::Incoming: Send + 'static,
{
    type Io = TlsStream<T::Io, ServerSession>;
    type Error = CritError;
    type Incoming = Box<dyn Stream<Item = Self::Io, Error = Self::Error> + Send + 'static>;
    type Data = T::Data;

    fn incoming(self) -> io::Result<Self::Incoming> {
        let Self {
            acceptor,
            raw_transport,
        } = self;

        let incoming = raw_transport.incoming()?;
        let incoming = Box::new(
            incoming
                .map(move |stream| acceptor.accept(stream).map_err(Into::into).into_stream())
                .map_err(Into::into)
                .flatten(),
        );

        Ok(incoming)
    }
}

impl<Io, S> HasConnectionInfo for TlsStream<Io, S>
where
    Io: HasConnectionInfo,
{
    type Data = Io::Data;
    type Info = Io::Info;

    fn fetch_info(&self) -> io::Result<Self::Info> {
        self.get_ref().0.fetch_info()
    }
}
