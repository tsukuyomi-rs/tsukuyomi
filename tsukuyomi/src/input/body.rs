//! Components for receiving incoming request bodies.

use {
    super::localmap::{local_key, LocalData},
    bytes::{Buf, BufMut, Bytes, BytesMut},
    futures01::{Async, Future, Poll, Stream},
    http::header::HeaderMap,
    hyper::body::{Body, Payload},
    std::{fmt, io, mem},
};

#[derive(Debug)]
pub struct RequestBody(Body);

impl RequestBody {
    #[inline]
    pub fn on_upgrade(self) -> OnUpgrade {
        OnUpgrade(self.0.on_upgrade())
    }

    pub(crate) fn into_inner(self) -> Body {
        self.0
    }

    #[doc(hidden)]
    #[deprecated(
        since = "0.5.3",
        note = "this method will be removed in the next version."
    )]
    #[allow(deprecated)]
    pub fn read_all(self) -> ReadAll {
        ReadAll::new(self)
    }
}

impl LocalData for RequestBody {
    local_key! {
        /// The local key to manage the request body
        /// stored in the current context.
        const KEY: Self;
    }
}

impl From<Body> for RequestBody {
    fn from(body: Body) -> Self {
        RequestBody(body)
    }
}

impl Payload for RequestBody {
    type Data = hyper::Chunk;
    type Error = hyper::Error;

    #[inline]
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        self.0.poll_data()
    }

    #[inline]
    fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
        self.0.poll_trailers()
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    #[inline]
    fn content_length(&self) -> Option<u64> {
        self.0.content_length()
    }
}

impl Stream for RequestBody {
    type Item = hyper::Chunk;
    type Error = hyper::Error;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_data()
    }
}

/// An asynchronous I/O upgraded from HTTP connection.
///
/// Currenly, this type is implemented as a thin wrapper of `hyper::upgrade::Upgraded`.
#[derive(Debug)]
pub struct UpgradedIo(hyper::upgrade::Upgraded);

impl io::Read for UpgradedIo {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for UpgradedIo {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl tokio_io::AsyncRead for UpgradedIo {
    #[inline]
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        tokio_io::AsyncRead::prepare_uninitialized_buffer(&self.0, buf)
    }

    #[inline]
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        tokio_io::AsyncRead::read_buf(&mut self.0, buf)
    }
}

impl tokio_io::AsyncWrite for UpgradedIo {
    #[inline]
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        tokio_io::AsyncWrite::shutdown(&mut self.0)
    }

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        tokio_io::AsyncWrite::write_buf(&mut self.0, buf)
    }
}

#[derive(Debug)]
pub struct OnUpgrade(hyper::upgrade::OnUpgrade);

impl Future for OnUpgrade {
    type Item = UpgradedIo;
    type Error = hyper::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map(|x| x.map(UpgradedIo))
    }
}

// ==== ReadAll ====

#[doc(hidden)]
#[deprecated(
    since = "0.5.3",
    note = "this struct will be removed in the next version."
)]
#[must_use = "futures do nothing unless polled"]
pub struct ReadAll {
    state: ReadAllState,
}

#[allow(deprecated)]
impl fmt::Debug for ReadAll {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadAll")
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum ReadAllState {
    Receiving(RequestBody, BytesMut),
    Done,
}

#[allow(deprecated)]
impl ReadAll {
    fn new(body: RequestBody) -> Self {
        Self {
            state: ReadAllState::Receiving(body, BytesMut::new()),
        }
    }
}

#[allow(deprecated)]
impl Future for ReadAll {
    type Item = Bytes;
    type Error = hyper::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ReadAllState::*;
        loop {
            match self.state {
                Receiving(ref mut body, ref mut buf) => {
                    while let Some(chunk) = futures01::try_ready!(body.poll_data()) {
                        buf.extend_from_slice(&*chunk);
                    }
                }
                Done => panic!("cannot resolve twice"),
            }

            match mem::replace(&mut self.state, Done) {
                Receiving(_body, buf) => {
                    // debug_assert!(body.is_end_stream());
                    return Ok(Async::Ready(buf.freeze()));
                }
                Done => unreachable!(),
            }
        }
    }
}
