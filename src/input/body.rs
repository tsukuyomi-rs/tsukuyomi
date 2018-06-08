use bytes::{Buf, Bytes, BytesMut};
use futures::{Future, Poll, Stream};
use http::header::HeaderMap;
use hyper::body::{self, Body, Payload as _Payload};
use std::cell::RefCell;
use std::mem;
use std::ops::Deref;

use error::CritError;

// ==== RequestBody ====

#[derive(Debug)]
pub struct RequestBody(RefCell<Option<Body>>);

impl RequestBody {
    pub(crate) fn from_hyp(body: Body) -> RequestBody {
        RequestBody(RefCell::new(Some(body)))
    }

    pub(crate) fn into_hyp(self) -> Body {
        self.0.borrow_mut().take().unwrap_or_default()
    }

    pub fn is_gone(&self) -> bool {
        self.0.borrow().is_none()
    }

    pub fn payload(&self) -> Option<Payload> {
        self.0.borrow_mut().take().map(Payload)
    }

    pub fn read_all(&self) -> ReadAll {
        let body = self.0.borrow_mut().take();
        ReadAll {
            state: ReadAllState::Init(body),
        }
    }
}

// ==== Payload ====

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

#[derive(Debug)]
pub struct Chunk(body::Chunk);

impl Chunk {
    fn from_hyp(chunk: body::Chunk) -> Chunk {
        Chunk(chunk)
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
                Receiving(body, buf) => {
                    debug_assert!(body.is_end_stream());
                    return Ok(buf.freeze().into());
                }
                Done => unreachable!(),
            }
        }
    }
}
