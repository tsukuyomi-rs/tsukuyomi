use bytes::Bytes;
use failure::Error;
use futures::{Future, Poll};
use hyper::service::Service;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait ServiceUpgradeExt<I: AsyncRead + AsyncWrite>: Service + Sized {
    type Upgrade: Future<Item = (), Error = ()>;
    type UpgradeError: Into<Error>;

    fn poll_ready_upgradable(&mut self) -> Poll<(), Self::UpgradeError>;

    fn upgrade(self, io: I, read_buf: Bytes) -> Self::Upgrade;
}
