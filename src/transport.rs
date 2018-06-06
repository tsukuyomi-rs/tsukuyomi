use futures::{Poll, Stream};
use std::io;
use std::net::SocketAddr;
use tokio::net::{self as tokio_net, TcpListener, TcpStream};

pub type Io = TcpStream;

#[derive(Debug)]
pub struct Incoming {
    inner: tokio_net::Incoming,
}

impl Incoming {
    pub fn new(addr: &SocketAddr) -> io::Result<Incoming> {
        Ok(Incoming {
            inner: TcpListener::bind(addr)?.incoming(),
        })
    }
}

impl Stream for Incoming {
    type Item = Io;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.inner.poll()
    }
}
