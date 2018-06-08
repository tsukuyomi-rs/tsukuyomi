use bytes::{Buf, BufMut};
use futures::Poll;
#[cfg(feature = "tls")]
use rustls::ServerSession;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
#[cfg(feature = "tls")]
use tokio_rustls::TlsStream;
use tokio_tcp::TcpStream;
#[cfg(unix)]
use tokio_uds::UnixStream;

#[derive(Debug)]
pub enum MaybeTls<S> {
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
    pub fn session(&self) -> Option<&ServerSession> {
        match *self {
            #[cfg(feature = "tls")]
            MaybeTls::Tls(ref s) => Some(s.get_ref().1),
            _ => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut ServerSession> {
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

impl Io {
    pub(crate) fn tcp(stream: MaybeTls<TcpStream>) -> Io {
        Io(IoKind::Tcp(stream))
    }

    #[cfg(unix)]
    pub(crate) fn uds(stream: MaybeTls<UnixStream>) -> Io {
        Io(IoKind::Uds(stream))
    }
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
