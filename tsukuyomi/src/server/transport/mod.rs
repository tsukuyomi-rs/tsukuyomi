mod tcp;
mod tls;
mod uds;

#[cfg(feature = "tls")]
pub use self::tls::{tls, TlsConfig};

use futures::Stream;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait HasConnectionInfo {
    type Data;
    type Info: ConnectionInfo<Data = Self::Data> + Send + 'static;

    fn fetch_info(&self) -> io::Result<Self::Info>;
}

pub trait ConnectionInfo {
    type Data;

    fn data(&self) -> Self::Data;
}

/// A trait representing the low-level I/O used in `Server`.
pub trait Transport {
    type Io: AsyncRead + AsyncWrite + HasConnectionInfo<Data = Self::Data>;
    type Error;
    type Data;
    type Incoming: Stream<Item = Self::Io, Error = Self::Error>;

    /// Creates a `Stream` of asynchronous I/Os.
    fn incoming(self) -> io::Result<Self::Incoming>;
}

/// A wrapper type containing a peer address.
#[derive(Debug)]
pub struct Peer<T>(T);

impl<T> std::ops::Deref for Peer<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
