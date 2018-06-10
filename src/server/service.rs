use bytes::Bytes;
use failure::Error;
use futures::{Future, Poll};
use hyper::service::Service;
use tokio::io::{AsyncRead, AsyncWrite};

/// A trait for extending `Service` by adding some methods and associated types required by
/// the protocol upgrade.
pub trait ServiceUpgradeExt<I: AsyncRead + AsyncWrite>: Service + Sized {
    /// A future returned from `into_upgrade`, representing an asynchronous computation
    /// after upgrading to another protocol.
    type Upgrade: Future<Item = (), Error = ()>;

    /// The type of error which will be returned from `poll_ready_upgradable`.
    type UpgradeError: Into<Error>;

    /// Polls if this service is upgradable.
    fn poll_ready_upgradable(&mut self) -> Poll<(), Self::UpgradeError>;

    /// Perform the upgrading to another protocol with the provided components.
    ///
    /// Implementors of this method are responsible for shutting down the provided IO.
    fn upgrade(self, io: I, read_buf: Bytes) -> Self::Upgrade;
}
