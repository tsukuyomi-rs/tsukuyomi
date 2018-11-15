use std::io;

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

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait HasConnectionInfo {
    type Data;
    type Error;
    type Info: ConnectionInfo<Data = Self::Data>;

    fn fetch_info(&self) -> Result<Self::Info, Self::Error>;
}

pub trait ConnectionInfo {
    type Data;

    fn data(&self) -> Self::Data;
}

mod tcp {
    use super::*;
    use std::net::SocketAddr;
    use tokio::net::TcpStream;

    impl HasConnectionInfo for TcpStream {
        type Data = Peer<SocketAddr>;
        type Error = io::Error;
        type Info = TcpConnectionInfo;

        #[inline]
        fn fetch_info(&self) -> io::Result<Self::Info> {
            Ok(TcpConnectionInfo {
                peer_addr: self.peer_addr()?,
            })
        }
    }

    #[allow(missing_debug_implementations)]
    #[cfg_attr(feature = "cargo-clippy", allow(stutter))]
    pub struct TcpConnectionInfo {
        peer_addr: SocketAddr,
    }

    impl ConnectionInfo for TcpConnectionInfo {
        type Data = Peer<SocketAddr>;

        fn data(&self) -> Self::Data {
            Peer(self.peer_addr)
        }
    }
}

#[cfg(unix)]
mod uds {
    use super::*;

    use std::os::unix::net::SocketAddr;
    use tokio::net::UnixStream;

    impl HasConnectionInfo for UnixStream {
        type Data = Peer<SocketAddr>;
        type Error = io::Error;
        type Info = UdsConnectionInfo;

        #[inline]
        fn fetch_info(&self) -> io::Result<Self::Info> {
            Ok(UdsConnectionInfo {
                peer_addr: self.peer_addr()?,
            })
        }
    }

    #[allow(missing_debug_implementations)]
    #[cfg_attr(feature = "cargo-clippy", allow(stutter))]
    pub struct UdsConnectionInfo {
        peer_addr: SocketAddr,
    }

    impl ConnectionInfo for UdsConnectionInfo {
        type Data = Peer<SocketAddr>;

        fn data(&self) -> Self::Data {
            Peer(self.peer_addr.clone())
        }
    }
}
