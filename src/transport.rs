use bytes::{Buf, BufMut};
use futures::{Poll, Stream};
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tcp::{self as tcp, TcpListener, TcpStream};
#[cfg(unix)]
use tokio_uds::{self as uds, UnixListener, UnixStream};

#[derive(Debug)]
pub struct Io(IoKind);

#[derive(Debug)]
enum IoKind {
    RawTcp(TcpStream),
    RawUds(UnixStream),
}

macro_rules! impl_io {
    ($self:expr, $s:ident => $e:expr) => {
        match $self.0 {
            IoKind::RawTcp(ref $s) => $e,
            #[cfg(unix)]
            IoKind::RawUds(ref $s) => $e,
        }
    };
    (@mut $self:expr, $s:ident => $e:expr) => {
        match $self.0 {
            IoKind::RawTcp(ref mut $s) => $e,
            #[cfg(unix)]
            IoKind::RawUds(ref mut $s) => $e,
        }
    };
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

#[derive(Debug)]
pub struct Incoming(IncomingKind);

#[derive(Debug)]
enum IncomingKind {
    Tcp(tcp::Incoming),
    #[cfg(unix)]
    Uds(uds::Incoming),
}

impl Incoming {
    pub fn tcp(addr: &SocketAddr) -> io::Result<Incoming> {
        Ok(Incoming(IncomingKind::Tcp(
            TcpListener::bind(addr)?.incoming(),
        )))
    }

    pub fn uds<P>(path: P) -> io::Result<Incoming>
    where
        P: AsRef<Path>,
    {
        Ok(Incoming(IncomingKind::Uds(
            UnixListener::bind(path)?.incoming(),
        )))
    }
}

impl Stream for Incoming {
    type Item = Io;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.0 {
            IncomingKind::Tcp(ref mut i) => i.poll()
                .map(|i| i.map(|stream| stream.map(|stream| Io(IoKind::RawTcp(stream))))),
            #[cfg(unix)]
            IncomingKind::Uds(ref mut i) => i.poll()
                .map(|i| i.map(|stream| stream.map(|stream| Io(IoKind::RawUds(stream))))),
        }
    }
}
