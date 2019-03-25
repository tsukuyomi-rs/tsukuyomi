//! Abstraction of HTTP upgrade in Tsukuyomi.

use {
    crate::util::{Either, Never}, //
    futures01::Poll,
    std::{any::TypeId, fmt, io},
    tokio_io::{AsyncRead, AsyncWrite},
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// A trait that abstracts asynchronous tasks to be ran after upgrading the protocol.
pub trait Upgrade {
    /// Polls the completion of this task with the provided I/O.
    fn poll_upgrade(&mut self, io: &mut Upgraded<'_>) -> Poll<(), Error>;

    /// Notifies that the task is to be shutdown.
    fn close(&mut self);
}

#[allow(missing_debug_implementations)]
pub struct NeverUpgrade(Never);

impl Upgrade for NeverUpgrade {
    fn poll_upgrade(&mut self, _: &mut Upgraded<'_>) -> Poll<(), Error> {
        match self.0 {}
    }

    fn close(&mut self) {
        match self.0 {}
    }
}

impl<L, R> Upgrade for Either<L, R>
where
    L: Upgrade,
    R: Upgrade,
{
    fn poll_upgrade(&mut self, io: &mut Upgraded<'_>) -> Poll<(), Error> {
        match self {
            Either::Left(l) => l.poll_upgrade(io),
            Either::Right(r) => r.poll_upgrade(io),
        }
    }

    fn close(&mut self) {
        match self {
            Either::Left(l) => l.close(),
            Either::Right(r) => r.close(),
        }
    }
}

// ===== Upgraded =====

/// A proxy for accessing an upgraded I/O from `Upgrade`.
pub struct Upgraded<'a>(&'a mut dyn Io);

impl<'a> fmt::Debug for Upgraded<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upgraded").finish()
    }
}

impl<'a> io::Read for Upgraded<'a> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.0.read(dst)
    }
}

impl<'a> io::Write for Upgraded<'a> {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.0.write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<'a> AsyncRead for Upgraded<'a> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }
}

impl<'a> AsyncWrite for Upgraded<'a> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.0.shutdown()
    }
}

impl<'a> Upgraded<'a> {
    pub(crate) fn new<I>(io: &'a mut I) -> Self
    where
        I: AsyncRead + AsyncWrite + 'static,
    {
        Upgraded(io)
    }

    /// Attempts to downcast the inner value to the specified concrete type.
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: AsyncRead + AsyncWrite + 'static,
    {
        if self.0.is::<T>() {
            unsafe { Some(self.0.downcast_ref_unchecked()) }
        } else {
            None
        }
    }

    /// Attempts to downcast the inner value to the specified concrete type.
    pub fn downcast_mut<T>(&mut self) -> Option<&mut T>
    where
        T: AsyncRead + AsyncWrite + 'static,
    {
        if self.0.is::<T>() {
            unsafe { Some(self.0.downcast_mut_unchecked()) }
        } else {
            None
        }
    }
}

trait Io: AsyncRead + AsyncWrite + 'static {
    #[doc(hidden)]
    fn __type_id__(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl<I: AsyncRead + AsyncWrite + 'static> Io for I {}

impl dyn Io {
    fn is<T: Io>(&self) -> bool {
        self.__type_id__() == TypeId::of::<T>()
    }

    unsafe fn downcast_ref_unchecked<T: Io>(&self) -> &T {
        &*(self as *const Self as *const T)
    }

    unsafe fn downcast_mut_unchecked<T: Io>(&mut self) -> &mut T {
        &mut *(self as *mut Self as *mut T)
    }
}
