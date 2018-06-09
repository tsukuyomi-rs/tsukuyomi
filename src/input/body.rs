use bytes::{Buf, Bytes, BytesMut};
use futures::{Future, Poll, Stream};
use http::header::HeaderMap;
use http::Request;
use hyper::body::{self, Body, Payload as _Payload};
use std::cell::UnsafeCell;
use std::mem;
use std::ops::Deref;

use context::Context;
use error::{CritError, Error};

// ==== RequestBody ====

/// A type representing a message body in the incoming HTTP request.
///
/// NOTE: This type has the internal mutability in order to extract the instance of raw message body
/// without a mutable borrow.
#[derive(Debug)]
pub struct RequestBody(UnsafeCell<Option<Body>>);

impl RequestBody {
    pub(crate) fn from_hyp(body: Body) -> RequestBody {
        RequestBody(UnsafeCell::new(Some(body)))
    }

    fn take_body(&self) -> Option<Body> {
        // safety: this type does not shared between threads and the following
        // mutable reference is used only this block.
        unsafe {
            let body = &mut *self.0.get();
            body.take()
        }
    }

    /// Takes away the instance of raw message body if exists.
    pub fn forget(&self) {
        self.take_body().map(mem::drop);
    }

    /// Returns 'true' if the instance of raw message body has already taken away.
    pub fn is_gone(&self) -> bool {
        // safety: this type does not shared between threads and the following
        // shared reference is used only this block.
        unsafe {
            let body = &*self.0.get();
            body.is_none()
        }
    }

    /// Creates an instance of "Payload" from the raw message body.
    pub fn payload(&self) -> Option<Payload> {
        self.take_body().map(Payload)
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&self) -> ReadAll {
        ReadAll {
            state: ReadAllState::Init(self.take_body()),
        }
    }
}

// ==== Payload ====

/// Raw streaming body of incoming HTTP requests.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Payload(Body);

impl Payload {
    pub fn poll_data(&mut self) -> Poll<Option<Chunk>, CritError> {
        self.0
            .poll_data()
            .map(|x| x.map(|c| c.map(Chunk::from_hyp)))
            .map_err(Into::into)
    }

    pub fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, CritError> {
        self.0.poll_trailers().map_err(Into::into)
    }

    pub fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.0.content_length()
    }
}

impl body::Payload for Payload {
    type Data = Chunk;
    type Error = CritError;

    #[inline]
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        self.poll_data()
    }

    #[inline]
    fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
        self.poll_trailers()
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.is_end_stream()
    }

    #[inline]
    fn content_length(&self) -> Option<u64> {
        self.content_length()
    }
}

impl Stream for Payload {
    type Item = Chunk;
    type Error = CritError;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_data()
    }
}

/// A buffer of bytes which will be returned from `Payload`.
#[derive(Debug)]
pub struct Chunk(pub(crate) body::Chunk);

impl Chunk {
    fn from_hyp(chunk: body::Chunk) -> Chunk {
        Chunk(chunk)
    }

    /// Converts itself into a `Byte`.
    pub fn into_bytes(self) -> Bytes {
        self.0.into_bytes()
    }
}

impl Into<Bytes> for Chunk {
    fn into(self) -> Bytes {
        self.into_bytes()
    }
}

impl AsRef<[u8]> for Chunk {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Deref for Chunk {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl IntoIterator for Chunk {
    type Item = u8;
    type IntoIter = <body::Chunk as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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

// ==== ReadAll ====

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct ReadAll {
    state: ReadAllState,
}

impl ReadAll {
    pub fn convert_to<T>(self) -> impl Future<Item = T, Error = Error> + Send + 'static
    where
        T: FromData + Send,
    {
        self.map_err(Error::critical)
            .and_then(|body| Context::with(|cx| T::from_data(body, cx.request())))
    }
}

#[derive(Debug)]
enum ReadAllState {
    Init(Option<Body>),
    Receiving(Body, BytesMut),
    Done,
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ReadAllState::*;
        loop {
            match self.state {
                Init(..) => {}
                Receiving(ref mut body, ref mut buf) => {
                    while let Some(chunk) = try_ready!(body.poll_data()) {
                        buf.extend_from_slice(&*chunk);
                    }
                }
                Done => panic!("cannot resolve twice"),
            }

            match mem::replace(&mut self.state, Done) {
                Init(body) => {
                    let body = body.ok_or_else(|| format_err!("").compat())?;
                    self.state = Receiving(body, BytesMut::new());
                    continue;
                }
                Receiving(_body, buf) => {
                    // debug_assert!(body.is_end_stream());
                    return Ok(buf.freeze().into());
                }
                Done => unreachable!(),
            }
        }
    }
}

pub trait FromData: Sized + 'static {
    fn from_data<T>(data: Bytes, request: &Request<T>) -> Result<Self, Error>;
}
