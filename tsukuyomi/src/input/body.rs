//! Components for receiving incoming request bodies.

use {
    super::localmap::{local_key, LocalData},
    crate::error::HttpError,
    bytes::{Buf, BufMut, Bytes, BytesMut},
    futures01::{Future, Poll, Stream},
    http::{Request, Response, StatusCode},
    izanami_util::{
        buf_stream::{BufStream, SizeHint},
        http::{Upgrade, Upgraded},
    },
    std::{fmt, io},
    tokio_io::{AsyncRead, AsyncWrite},
};

pub struct Chunk(Box<dyn Buf + Send + 'static>);

impl fmt::Debug for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chunk").finish()
    }
}

impl AsRef<[u8]> for Chunk {
    fn as_ref(&self) -> &[u8] {
        self.0.bytes()
    }
}

impl Buf for Chunk {
    fn remaining(&self) -> usize {
        self.0.remaining()
    }

    fn bytes(&self) -> &[u8] {
        self.0.bytes()
    }

    fn advance(&mut self, cnt: usize) {
        self.0.advance(cnt)
    }
}

pub struct Error(Box<dyn std::error::Error + Send + Sync + 'static>);

impl std::ops::Deref for Error {
    type Target = dyn std::error::Error + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl HttpError for Error {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        let mut response = Response::new(self.to_string());
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        response
    }
}

trait BoxedBufStream {
    fn poll_buf_boxed(&mut self) -> Poll<Option<Chunk>, Error>;
    fn size_hint_boxed(&self) -> SizeHint;
    fn consume_hint_boxed(&mut self, amt: usize);
}

impl<T> BoxedBufStream for T
where
    T: BufStream,
    T::Item: Send + 'static,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    fn poll_buf_boxed(&mut self) -> Poll<Option<Chunk>, Error> {
        self.poll_buf()
            .map(|x| x.map(|opt| opt.map(|buf| Chunk(Box::new(buf)))))
            .map_err(|e| Error(e.into()))
    }

    fn size_hint_boxed(&self) -> SizeHint {
        self.size_hint()
    }

    fn consume_hint_boxed(&mut self, amt: usize) {
        self.consume_hint(amt)
    }
}

trait BoxedUpgrade {
    fn poll_upgrade_boxed(&mut self) -> Poll<UpgradedIo, Error>;
}

impl<T> BoxedUpgrade for T
where
    T: Upgrade,
    T::Upgraded: Send + 'static,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    fn poll_upgrade_boxed(&mut self) -> Poll<UpgradedIo, Error> {
        self.poll_upgrade()
            .map(|x| x.map(|io| UpgradedIo(Box::new(io))))
            .map_err(|e| Error(e.into()))
    }
}

trait BoxedRequestBody: BoxedBufStream + BoxedUpgrade + Send + 'static {}
impl<T> BoxedRequestBody for T where T: BoxedBufStream + BoxedUpgrade + Send + 'static {}

#[allow(missing_debug_implementations)]
pub struct RequestBody(Box<dyn BoxedRequestBody>);

impl<T> From<T> for RequestBody
where
    T: BufStream + Upgrade + Send + 'static,
    <T as BufStream>::Item: Send + 'static,
    <T as BufStream>::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    <T as Upgrade>::Upgraded: Send + 'static,
    <T as Upgrade>::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    fn from(body: T) -> Self {
        RequestBody(Box::new(body))
    }
}

impl LocalData for RequestBody {
    local_key! {
        /// The local key to manage the request body
        /// stored in the current context.
        const KEY: Self;
    }
}

impl Stream for RequestBody {
    type Item = Chunk;
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_buf()
    }
}

impl BufStream for RequestBody {
    type Item = Chunk;
    type Error = Error;

    #[inline]
    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.0.poll_buf_boxed()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.0.size_hint_boxed()
    }
}

impl RequestBody {
    pub fn read_all(self) -> ReadAll {
        ReadAll {
            body: self,
            acc: BytesMut::new(),
        }
    }

    pub fn on_upgrade(self) -> OnUpgrade {
        OnUpgrade(self)
    }
}

#[allow(missing_debug_implementations)]
pub struct ReadAll {
    body: RequestBody,
    acc: BytesMut,
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Some(buf) = futures01::try_ready!(self.body.poll_buf()) {
            self.acc.put(buf);
        }

        let buf = std::mem::replace(&mut self.acc, BytesMut::new()).freeze();
        Ok(buf.into())
    }
}

#[allow(missing_debug_implementations)]
pub struct OnUpgrade(RequestBody);

impl Future for OnUpgrade {
    type Item = UpgradedIo;
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        (self.0).0.poll_upgrade_boxed()
    }
}

pub struct UpgradedIo(Box<dyn Upgraded + Send + 'static>);

impl UpgradedIo {
    pub fn downcast<T>(self) -> Result<T, Self>
    where
        T: AsyncRead + AsyncWrite + Send + 'static,
    {
        self.0.downcast().map(|boxed| *boxed).map_err(UpgradedIo)
    }
}

impl fmt::Debug for UpgradedIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpgradedIo").finish()
    }
}

impl std::ops::Deref for UpgradedIo {
    type Target = dyn Upgraded + Send + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl std::ops::DerefMut for UpgradedIo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

impl io::Read for UpgradedIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for UpgradedIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl AsyncRead for UpgradedIo {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.read_buf(buf)
    }
}

impl AsyncWrite for UpgradedIo {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.0.shutdown()
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.0.write_buf(buf)
    }
}
