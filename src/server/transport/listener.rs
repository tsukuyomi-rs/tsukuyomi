use failure::Error;
use futures::{Future, Poll, Stream};
#[cfg(feature = "tls")]
use rustls::{ServerConfig, ServerSession};
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(feature = "tls")]
use std::sync::Arc;
use std::{fmt, io, mem};
use tokio::io::{AsyncRead, AsyncWrite};
#[cfg(feature = "tls")]
use tokio_rustls::{self, AcceptAsync};
use tokio_tcp::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio_uds::{UnixListener, UnixStream};

use super::io::{Io, MaybeTls};
#[cfg(feature = "tls")]
use super::tls::{self, TlsConfig};

fn map_async<T, E, U>(x: Poll<T, E>, f: impl FnOnce(T) -> U) -> Poll<U, E> {
    x.map(|a| a.map(f))
}

#[derive(Debug)]
enum Config {
    Tcp {
        addr: SocketAddr,
    },
    #[cfg(unix)]
    Uds {
        path: PathBuf,
    },
}

impl Default for Config {
    fn default() -> Self {
        Config::Tcp {
            addr: ([127, 0, 0, 1], 4000).into(),
        }
    }
}

// ==== Builder ====

#[derive(Debug, Default)]
pub struct Builder {
    config: Config,
    #[cfg(feature = "tls")]
    tls: Option<TlsConfig>,
}

impl Builder {
    pub fn bind_tcp<A>(&mut self, addr: A) -> &mut Builder
    where
        A: Into<SocketAddr>,
    {
        self.config = Config::Tcp { addr: addr.into() };
        self
    }

    #[cfg(unix)]
    pub fn bind_uds<P>(&mut self, path: P) -> &mut Builder
    where
        P: Into<PathBuf>,
    {
        self.config = Config::Uds { path: path.into() };
        self
    }

    #[cfg(feature = "tls")]
    pub fn set_tls(&mut self, config: TlsConfig) -> &mut Builder {
        self.tls = Some(config);
        self
    }

    pub fn finish(&mut self) -> Result<Listener, Error> {
        let builder = mem::replace(self, Default::default());

        Ok(Listener {
            kind: match builder.config {
                Config::Tcp { addr } => ListenerKind::Tcp(TcpListener::bind(&addr)?),
                #[cfg(unix)]
                Config::Uds { path } => ListenerKind::Uds(UnixListener::bind(path)?),
            },
            #[cfg(feature = "tls")]
            tls: match builder.tls {
                Some(config) => Some(tls::load_config(&config).map(Arc::new)?),
                None => None,
            },
        })
    }
}

// ==== Listener ====

pub struct Listener {
    kind: ListenerKind,
    #[cfg(feature = "tls")]
    tls: Option<Arc<ServerConfig>>,
}

#[derive(Debug)]
enum ListenerKind {
    Tcp(TcpListener),
    #[cfg(unix)]
    Uds(UnixListener),
}

impl ListenerKind {
    fn poll_accept_raw(&mut self) -> Poll<Handshake, io::Error> {
        match *self {
            ListenerKind::Tcp(ref mut l) => map_async(l.poll_accept(), |(s, _)| Handshake::raw_tcp(s)),
            #[cfg(unix)]
            ListenerKind::Uds(ref mut l) => map_async(l.poll_accept(), |(s, _)| Handshake::raw_uds(s)),
        }
    }

    #[cfg(feature = "tls")]
    fn poll_accept_tls(&mut self, session: ServerSession) -> Poll<Handshake, io::Error> {
        match *self {
            ListenerKind::Tcp(ref mut l) => map_async(l.poll_accept(), |(s, _)| Handshake::tcp_with_tls(s, session)),
            #[cfg(unix)]
            ListenerKind::Uds(ref mut l) => map_async(l.poll_accept(), |(s, _)| Handshake::uds_with_tls(s, session)),
        }
    }
}

impl fmt::Debug for Listener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut d = f.debug_struct("Listener");
        d.field("kind", &self.kind);
        #[cfg(feature = "tls")]
        d.field("tls", &self.tls.as_ref().map(|_| "<TLS Config>"));
        d.finish()
    }
}

impl Listener {
    pub fn builder() -> Builder {
        Default::default()
    }

    #[cfg(not(feature = "tls"))]
    pub fn poll_accept(&mut self) -> Poll<Handshake, io::Error> {
        self.kind.poll_accept_raw()
    }

    #[cfg(feature = "tls")]
    pub fn poll_accept(&mut self) -> Poll<Handshake, io::Error> {
        match self.tls {
            Some(ref config) => {
                let session = ServerSession::new(config);
                self.kind.poll_accept_tls(session)
            }
            None => self.kind.poll_accept_raw(),
        }
    }

    pub fn incoming(self) -> Incoming {
        Incoming { listener: self }
    }
}

// ==== Incoming ====

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Incoming {
    listener: Listener,
}

impl Stream for Incoming {
    type Item = Handshake;
    type Error = io::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        map_async(self.listener.poll_accept(), Some)
    }
}

// ===== Handshake ====

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Handshake(HandshakeKind);

enum HandshakeKind {
    Tcp(MaybeTlsHandshake<TcpStream>),
    #[cfg(unix)]
    Uds(MaybeTlsHandshake<UnixStream>),
}

impl Handshake {
    fn raw_tcp(stream: TcpStream) -> Handshake {
        Handshake(HandshakeKind::Tcp(MaybeTlsHandshake::Raw(Some(stream))))
    }

    #[cfg(feature = "tls")]
    fn tcp_with_tls(stream: TcpStream, session: ServerSession) -> Handshake {
        Handshake(HandshakeKind::Tcp(MaybeTlsHandshake::Tls(
            tokio_rustls::accept_async_with_session(stream, session),
        )))
    }

    #[cfg(unix)]
    fn raw_uds(stream: UnixStream) -> Handshake {
        Handshake(HandshakeKind::Uds(MaybeTlsHandshake::Raw(Some(stream))))
    }

    #[cfg(unix)]
    #[cfg(feature = "tls")]
    fn uds_with_tls(stream: UnixStream, session: ServerSession) -> Handshake {
        Handshake(HandshakeKind::Uds(MaybeTlsHandshake::Tls(
            tokio_rustls::accept_async_with_session(stream, session),
        )))
    }
}

impl fmt::Debug for HandshakeKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HandshakeKind::Tcp(..) => f.debug_tuple("Tcp").finish(),
            #[cfg(unix)]
            HandshakeKind::Uds(..) => f.debug_tuple("Uds").finish(),
        }
    }
}

impl Future for Handshake {
    type Item = Io;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0 {
            HandshakeKind::Tcp(ref mut h) => map_async(h.poll(), Io::tcp),
            #[cfg(unix)]
            HandshakeKind::Uds(ref mut h) => map_async(h.poll(), Io::uds),
        }
    }
}

#[must_use = "futures do nothing unless polled"]
enum MaybeTlsHandshake<S> {
    Raw(Option<S>),
    #[cfg(feature = "tls")]
    Tls(AcceptAsync<S>),
}

impl<S: AsyncRead + AsyncWrite> Future for MaybeTlsHandshake<S> {
    type Item = MaybeTls<S>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match *self {
            MaybeTlsHandshake::Raw(ref mut s) => {
                let stream = s.take().expect("MaybeTlsHandshake has already resolved");
                Ok(MaybeTls::Raw(stream).into())
            }
            #[cfg(feature = "tls")]
            MaybeTlsHandshake::Tls(ref mut a) => map_async(a.poll(), MaybeTls::Tls),
        }
    }
}
