use {
    crate::util::{Either, Never}, //
    futures01::{Future, Poll},
};

pub trait Upgrade<S> {
    type Connection: Connection;

    fn upgrade(self, stream: S) -> Self::Connection;
}

#[allow(missing_debug_implementations)]
pub struct NeverUpgrade(Never);

impl<S> Upgrade<S> for NeverUpgrade {
    type Connection = Never;

    fn upgrade(self, _: S) -> Self::Connection {
        match self.0 {}
    }
}

impl<L, R, S> Upgrade<S> for Either<L, R>
where
    L: Upgrade<S>,
    R: Upgrade<S>,
{
    type Connection = Either<L::Connection, R::Connection>;

    fn upgrade(self, stream: S) -> Self::Connection {
        match self {
            Either::Left(l) => Either::Left(l.upgrade(stream)),
            Either::Right(r) => Either::Right(r.upgrade(stream)),
        }
    }
}

// ==== Connection ====

pub trait Connection {
    type Error: Into<Box<dyn std::error::Error + Send + Sync>>;

    fn poll_close(&mut self) -> Poll<(), Self::Error>;

    fn shutdown(&mut self);
}

impl Connection for Never {
    type Error = Never;

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        match *self {}
    }

    fn shutdown(&mut self) {
        match *self {}
    }
}

impl<F> Connection for F
where
    F: Future<Item = ()>,
    F::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Error = F::Error;

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        Future::poll(self)
    }

    fn shutdown(&mut self) {}
}

impl<L, R> Connection for Either<L, R>
where
    L: Connection,
    R: Connection,
{
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        match self {
            Either::Left(l) => l.poll_close().map_err(Into::into),
            Either::Right(r) => r.poll_close().map_err(Into::into),
        }
    }

    fn shutdown(&mut self) {
        match self {
            Either::Left(l) => l.shutdown(),
            Either::Right(r) => r.shutdown(),
        }
    }
}
