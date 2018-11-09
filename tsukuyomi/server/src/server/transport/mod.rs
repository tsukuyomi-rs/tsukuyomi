mod tcp;
mod tls;
mod uds;

pub use self::imp::Transport;

#[cfg(feature = "tls")]
pub use self::tls::{tls, TlsConfig};

// ==== impl ====
pub(crate) mod imp {
    use std::io;

    use futures::Stream;
    use http::Extensions;
    use tokio::io::{AsyncRead, AsyncWrite};

    use crate::server::CritError;

    pub trait HasConnectionInfo {
        type ConnectionInfo: ConnectionInfo;

        fn connection_info(&self) -> Self::ConnectionInfo;
    }

    pub trait ConnectionInfo {
        fn insert_info(&self, ext: &mut Extensions);
    }

    pub trait Transport: TransportImpl {}

    pub trait TransportImpl {
        type Info: ConnectionInfo + Send + 'static;
        type Io: AsyncRead
            + AsyncWrite
            + HasConnectionInfo<ConnectionInfo = Self::Info>
            + Send
            + 'static;
        type Error: Into<CritError>;
        type Incoming: Stream<Item = Self::Io, Error = Self::Error> + Send + 'static;

        fn incoming(self) -> io::Result<Self::Incoming>;
    }
}
