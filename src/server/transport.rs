use bytes::{Buf, BufMut};
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
use tokio_rustls::{self, AcceptAsync, TlsStream};
use tokio_tcp::{self as tcp, TcpListener, TcpStream};
#[cfg(unix)]
use tokio_uds::{self as uds, UnixListener, UnixStream};

// TODO: refactor

#[derive(Debug)]
enum MaybeTls<S> {
    Raw(S),
    #[cfg(feature = "tls")]
    Tls(TlsStream<S, ServerSession>),
}

macro_rules! impl_tls {
    ($self:expr, $s:ident => $e:expr) => {
        match *$self {
            MaybeTls::Raw(ref $s) => $e,
            #[cfg(feature = "tls")]
            MaybeTls::Tls(ref $s) => $e,
        }
    };
    (@mut $self:expr, $s:ident => $e:expr) => {
        match *$self {
            MaybeTls::Raw(ref mut $s) => $e,
            #[cfg(feature = "tls")]
            MaybeTls::Tls(ref mut $s) => $e,
        }
    };
}

#[cfg(feature = "tls")]
impl<S> MaybeTls<S> {
    fn session(&self) -> Option<&ServerSession> {
        match *self {
            #[cfg(feature = "tls")]
            MaybeTls::Tls(ref s) => Some(s.get_ref().1),
            _ => None,
        }
    }

    fn session_mut(&mut self) -> Option<&mut ServerSession> {
        match *self {
            #[cfg(feature = "tls")]
            MaybeTls::Tls(ref mut s) => Some(s.get_mut().1),
            _ => None,
        }
    }
}

impl<S: AsyncRead + AsyncWrite> io::Read for MaybeTls<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        impl_tls!(@mut self, s => s.read(buf))
    }
}

impl<S: AsyncRead + AsyncWrite> io::Write for MaybeTls<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        impl_tls!(@mut self, s => s.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        impl_tls!(@mut self, s => s.flush())
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncRead for MaybeTls<S> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        impl_tls!(self, s => s.prepare_uninitialized_buffer(buf))
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        impl_tls!(@mut self, s => s.read_buf(buf))
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for MaybeTls<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        impl_tls!(@mut self, s => s.shutdown())
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        impl_tls!(@mut self, s => s.write_buf(buf))
    }
}

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
            MaybeTlsHandshake::Tls(ref mut a) => a.poll().map(|a| a.map(|s| MaybeTls::Tls(s))),
        }
    }
}

// ==== Io ====

#[derive(Debug)]
pub struct Io(IoKind);

#[derive(Debug)]
enum IoKind {
    Tcp(MaybeTls<TcpStream>),
    #[cfg(unix)]
    Uds(MaybeTls<UnixStream>),
}

macro_rules! impl_io {
    ($self:expr, $s:ident => $e:expr) => {
        match $self.0 {
            IoKind::Tcp(ref $s) => $e,
            #[cfg(unix)]
            IoKind::Uds(ref $s) => $e,
        }
    };
    (@mut $self:expr, $s:ident => $e:expr) => {
        match $self.0 {
            IoKind::Tcp(ref mut $s) => $e,
            #[cfg(unix)]
            IoKind::Uds(ref mut $s) => $e,
        }
    };
}

#[cfg(feature = "tls")]
impl Io {
    pub fn session(&self) -> Option<&ServerSession> {
        impl_io!(self, s => s.session())
    }

    pub fn session_mut(&mut self) -> Option<&mut ServerSession> {
        impl_io!(@mut self, s => s.session_mut())
    }
}

impl io::Read for Io {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        impl_io!(@mut self, s => s.read(buf))
    }
}

impl io::Write for Io {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        impl_io!(@mut self, s => s.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        impl_io!(@mut self, s => s.flush())
    }
}

impl AsyncRead for Io {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        impl_io!(self, s => s.prepare_uninitialized_buffer(buf))
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        impl_io!(@mut self, s => s.read_buf(buf))
    }
}

impl AsyncWrite for Io {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        impl_io!(@mut self, s => s.shutdown())
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        impl_io!(@mut self, s => s.write_buf(buf))
    }
}

// ==== Incoming ====

#[derive(Debug)]
pub enum TransportConfig {
    Tcp {
        addr: SocketAddr,
    },
    #[cfg(unix)]
    Uds {
        path: PathBuf,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig::Tcp {
            addr: ([127, 0, 0, 1], 4000).into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Builder {
    config: TransportConfig,
    #[cfg(feature = "tls")]
    tls: Option<TlsConfig>,
}

impl Builder {
    pub fn set_transport(&mut self, config: TransportConfig) -> &mut Builder {
        self.config = config;
        self
    }

    #[cfg(feature = "tls")]
    pub fn set_tls(&mut self, config: TlsConfig) -> &mut Builder {
        self.tls = Some(config);
        self
    }

    pub fn finish(&mut self) -> Result<Incoming, Error> {
        let builder = mem::replace(self, Default::default());
        let kind = match builder.config {
            TransportConfig::Tcp { addr } => IncomingKind::Tcp(TcpListener::bind(&addr)?.incoming()),
            #[cfg(unix)]
            TransportConfig::Uds { path } => IncomingKind::Uds(UnixListener::bind(path)?.incoming()),
        };
        #[cfg(feature = "tls")]
        let tls = match builder.tls {
            Some(config) => Some(tls::load_config(&config).map(Arc::new)?),
            None => None,
        };
        Ok(Incoming {
            kind,
            #[cfg(feature = "tls")]
            tls,
        })
    }
}

pub struct Incoming {
    kind: IncomingKind,
    #[cfg(feature = "tls")]
    tls: Option<Arc<ServerConfig>>,
}

impl fmt::Debug for Incoming {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Incoming").field("kind", &self.kind).finish()
    }
}

#[derive(Debug)]
enum IncomingKind {
    Tcp(tcp::Incoming),
    #[cfg(unix)]
    Uds(uds::Incoming),
}

impl Incoming {
    pub fn builder() -> Builder {
        Default::default()
    }
}

#[cfg(feature = "tls")]
impl Stream for Incoming {
    type Item = Handshake;
    type Error = io::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.tls {
            Some(ref config) => poll_tls(&mut self.kind, config),
            None => poll_raw(&mut self.kind),
        }
    }
}

#[cfg(not(feature = "tls"))]
impl Stream for Incoming {
    type Item = Handshake;
    type Error = io::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        poll_raw(&mut self.kind)
    }
}

fn poll_raw(kind: &mut IncomingKind) -> Poll<Option<Handshake>, io::Error> {
    match *kind {
        IncomingKind::Tcp(ref mut i) => i.poll().map(|i| {
            i.map(|stream| stream.map(|stream| Handshake(HandshakeKind::Tcp(MaybeTlsHandshake::Raw(Some(stream))))))
        }),

        #[cfg(unix)]
        IncomingKind::Uds(ref mut i) => i.poll().map(|i| {
            i.map(|stream| stream.map(|stream| Handshake(HandshakeKind::Uds(MaybeTlsHandshake::Raw(Some(stream))))))
        }),
    }
}

#[cfg(feature = "tls")]
fn poll_tls(kind: &mut IncomingKind, config: &Arc<ServerConfig>) -> Poll<Option<Handshake>, io::Error> {
    let session = ServerSession::new(config);
    match *kind {
        IncomingKind::Tcp(ref mut i) => i.poll().map(|i| {
            i.map(|stream| {
                stream.map(|stream| {
                    Handshake(HandshakeKind::Tcp(MaybeTlsHandshake::Tls(
                        tokio_rustls::accept_async_with_session(stream, session),
                    )))
                })
            })
        }),

        #[cfg(unix)]
        IncomingKind::Uds(ref mut i) => i.poll().map(|i| {
            i.map(|stream| {
                stream.map(|stream| {
                    Handshake(HandshakeKind::Uds(MaybeTlsHandshake::Tls(
                        tokio_rustls::accept_async_with_session(stream, session),
                    )))
                })
            })
        }),
    }
}

// =====

#[derive(Debug)]
pub struct Handshake(HandshakeKind);

enum HandshakeKind {
    Tcp(MaybeTlsHandshake<TcpStream>),
    #[cfg(unix)]
    Uds(MaybeTlsHandshake<UnixStream>),
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
            HandshakeKind::Tcp(ref mut h) => h.poll().map(|a| a.map(|io| Io(IoKind::Tcp(io)))),
            #[cfg(unix)]
            HandshakeKind::Uds(ref mut h) => h.poll().map(|a| a.map(|io| Io(IoKind::Uds(io)))),
        }
    }
}

#[cfg(feature = "tls")]
pub use self::tls::TlsConfig;

#[cfg(feature = "tls")]
mod tls {
    use failure::Error;
    use rustls::internal::pemfile;
    use rustls::{Certificate, PrivateKey};
    use std::path::PathBuf;
    use std::{fs, io};

    pub use rustls::{ServerConfig, ServerSession};
    pub use tokio_rustls::{AcceptAsync, TlsStream};

    #[derive(Debug)]
    pub struct TlsConfig {
        pub certs_path: PathBuf,
        pub key_path: PathBuf,
        pub alpn_protocols: Vec<String>,
    }

    pub fn load_config(config: &TlsConfig) -> Result<ServerConfig, Error> {
        let certs = load_certs(&config.certs_path)?;
        let key = load_key(&config.key_path)?;

        let mut cfg = ServerConfig::new();
        cfg.set_single_cert(certs, key);
        cfg.set_protocols(&config.alpn_protocols[..]);

        Ok(cfg)
    }

    fn load_certs(path: &PathBuf) -> Result<Vec<Certificate>, Error> {
        let certfile = fs::File::open(path)?;
        let mut reader = io::BufReader::new(certfile);
        let certs = pemfile::certs(&mut reader).map_err(|_| format_err!("failed to read certificates"))?;
        Ok(certs)
    }

    fn load_key(path: &PathBuf) -> Result<PrivateKey, Error> {
        let keyfile = fs::File::open(path)?;
        let mut reader = io::BufReader::new(keyfile);
        let keys = pemfile::pkcs8_private_keys(&mut reader).map_err(|_| format_err!("failed to read private key"))?;
        if keys.is_empty() {
            bail!("empty private key");
        }
        Ok(keys[0].clone())
    }
}
