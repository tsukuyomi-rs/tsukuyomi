//! Components for receiving incoming request bodies.

use std::io;
use std::mem;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::{Async, Future, Poll, Stream};
use http::header::HeaderMap;
use hyper::body::{Body, Payload};

use crate::error::Critical;

#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct RequestBody(Body);

impl RequestBody {
    #[inline]
    pub(crate) fn on_upgrade(self) -> OnUpgrade {
        OnUpgrade(self.0.on_upgrade())
    }

    pub(crate) fn into_inner(self) -> Body {
        self.0
    }
}

impl From<Body> for RequestBody {
    fn from(body: Body) -> Self {
        RequestBody(body)
    }
}

impl Payload for RequestBody {
    type Data = io::Cursor<Bytes>;
    type Error = Critical;

    #[inline]
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        self.0
            .poll_data()
            .map(|x| x.map(|data_opt| data_opt.map(|data| io::Cursor::new(data.into_bytes()))))
            .map_err(Critical::new)
    }

    #[inline]
    fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
        self.0.poll_trailers().map_err(Critical::new)
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
    type Item = io::Cursor<Bytes>;
    type Error = Critical;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_data()
    }
}

#[cfg(feature = "tower-middleware")]
mod tower {
    use super::*;

    use tower_web::util::BufStream;

    impl BufStream for RequestBody {
        type Item = hyper::Chunk;
        type Error = hyper::Error;

        #[inline]
        fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
            BufStream::poll(&mut self.0)
        }

        fn size_hint(&self) -> tower_web::util::buf_stream::SizeHint {
            self.0.size_hint()
        }
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

impl tokio::io::AsyncRead for UpgradedIo {
    #[inline]
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        tokio::io::AsyncRead::prepare_uninitialized_buffer(&self.0, buf)
    }

    #[inline]
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        tokio::io::AsyncRead::read_buf(&mut self.0, buf)
    }
}

impl tokio::io::AsyncWrite for UpgradedIo {
    #[inline]
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        tokio::io::AsyncWrite::shutdown(&mut self.0)
    }

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        tokio::io::AsyncWrite::write_buf(&mut self.0, buf)
    }
}

#[derive(Debug)]
pub(crate) struct OnUpgrade(hyper::upgrade::OnUpgrade);

impl Future for OnUpgrade {
    type Item = UpgradedIo;
    type Error = Critical;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0
            .poll()
            .map(|x| x.map(UpgradedIo))
            .map_err(Critical::new)
    }
}

// ==== ReadAll ====

/// A future to receive the entire of incoming message body.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct ReadAll {
    state: ReadAllState,
}

#[derive(Debug)]
enum ReadAllState {
    Receiving(RequestBody, BytesMut),
    Done,
}

impl ReadAll {
    pub(super) fn new(body: RequestBody) -> Self {
        Self {
            state: ReadAllState::Receiving(body, BytesMut::new()),
        }
    }
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = Critical;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ReadAllState::*;
        loop {
            match self.state {
                Receiving(ref mut body, ref mut buf) => {
                    while let Some(chunk) = futures::try_ready!(body.poll_data()) {
                        let chunk = chunk.into_inner();
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
