use bytes::{Buf, Bytes, BytesMut};
use futures::{Future, Poll, Stream};
use http::header::HeaderMap;
use hyper::body::{self, Body, Payload};
use std::mem;
use std::ops::Deref;

use error::CritError;

#[derive(Debug)]
pub struct RequestBody(Body);

impl RequestBody {
    pub(crate) fn from_hyp(body: Body) -> RequestBody {
        RequestBody(body)
    }

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

    pub fn read_all(self) -> ReadAll {
        ReadAll {
            state: ReadAllState::Init(self.0),
        }
    }
}

impl Payload for RequestBody {
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

impl Stream for RequestBody {
    type Item = Chunk;
    type Error = CritError;

    #[inline]
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_data()
    }
}

// ==== ReadAll ====

#[derive(Debug)]
pub struct ReadAll {
    state: ReadAllState,
}

#[derive(Debug)]
enum ReadAllState {
    Init(Body),
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

// === Chunk ===

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
