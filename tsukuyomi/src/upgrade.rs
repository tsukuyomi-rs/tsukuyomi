use {
    crate::util::{Either, Never}, //
    futures01::Poll,
    tokio_io::{AsyncRead, AsyncWrite},
};

pub trait Upgraded: AsyncRead + AsyncWrite + 'static {
    // TODO: downcasting
}

impl<I: AsyncRead + AsyncWrite + 'static> Upgraded for I {}

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub trait Upgrade {
    fn poll_close(&mut self, stream: &mut dyn Upgraded) -> Poll<(), Error>;

    fn shutdown(&mut self);
}

#[allow(missing_debug_implementations)]
pub struct NeverUpgrade(Never);

impl Upgrade for NeverUpgrade {
    fn poll_close(&mut self, _: &mut dyn Upgraded) -> Poll<(), Error> {
        match self.0 {}
    }

    fn shutdown(&mut self) {
        match self.0 {}
    }
}

impl<L, R> Upgrade for Either<L, R>
where
    L: Upgrade,
    R: Upgrade,
{
    fn poll_close(&mut self, stream: &mut dyn Upgraded) -> Poll<(), Error> {
        match self {
            Either::Left(l) => l.poll_close(stream),
            Either::Right(r) => r.poll_close(stream),
        }
    }

    fn shutdown(&mut self) {
        match self {
            Either::Left(l) => l.shutdown(),
            Either::Right(r) => r.shutdown(),
        }
    }
}
