use std::borrow::Cow;
use std::mem;
use std::str;

use bytes::{Buf, Bytes};
use futures::{Async, Poll};
use http::header::HeaderMap;
use hyper::body::Payload;

#[derive(Debug)]
pub(super) struct Receive<Bd: Payload> {
    state: ReceiveState<Bd>,
    content_length: Option<u64>,
}

#[derive(Debug)]
enum ReceiveState<Bd: Payload> {
    Init(Bd),
    InProgress {
        body: Bd,
        chunks: Vec<Bytes>,
        end_of_chunks: bool,
    },
    Done(TestOutput),
    Gone,
}

impl<Bd: Payload> Receive<Bd> {
    pub(super) fn new(body: Bd) -> Receive<Bd> {
        let content_length = body.content_length();
        Receive {
            state: ReceiveState::Init(body),
            content_length,
        }
    }

    pub(super) fn poll_ready(&mut self) -> Poll<(), Bd::Error> {
        loop {
            let trailers = match self.state {
                ReceiveState::Init(..) => None,
                ReceiveState::InProgress {
                    ref mut body,
                    ref mut chunks,
                    ref mut end_of_chunks,
                } => {
                    if !*end_of_chunks {
                        while let Some(chunk) = futures::try_ready!(body.poll_data()) {
                            chunks.push(chunk.collect());
                        }
                        *end_of_chunks = true;
                        continue;
                    } else {
                        futures::try_ready!(body.poll_trailers())
                    }
                }
                ReceiveState::Done(..) => return Ok(Async::Ready(())),
                ReceiveState::Gone => panic!("The future has already polled"),
            };

            let old_state = mem::replace(&mut self.state, ReceiveState::Gone);
            match old_state {
                ReceiveState::Init(body) => {
                    self.state = ReceiveState::InProgress {
                        body,
                        chunks: vec![],
                        end_of_chunks: false,
                    };
                }
                ReceiveState::InProgress {
                    chunks,
                    end_of_chunks,
                    ..
                } => {
                    debug_assert!(end_of_chunks);
                    self.state = ReceiveState::Done(TestOutput {
                        chunks,
                        trailers,
                        content_length: self.content_length,
                    });
                    return Ok(Async::Ready(()));
                }
                ReceiveState::Done(..) | ReceiveState::Gone => unreachable!("unexpected condition"),
            }
        }
    }

    pub(super) fn into_data(self) -> Option<TestOutput> {
        match self.state {
            ReceiveState::Done(data) => Some(data),
            _ => None,
        }
    }
}

/// A type representing a received HTTP message data from the server.
///
/// This type is usually used by the testing framework.
#[derive(Debug)]
pub struct TestOutput {
    chunks: Vec<Bytes>,
    trailers: Option<HeaderMap>,
    content_length: Option<u64>,
}

#[allow(missing_docs)]
impl TestOutput {
    pub fn chunks(&self) -> &Vec<Bytes> {
        &self.chunks
    }

    pub fn trailers(&self) -> Option<&HeaderMap> {
        self.trailers.as_ref()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.content_length
    }

    pub fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.chunks().iter().fold(Vec::new(), |mut acc, chunk| {
            acc.extend_from_slice(&*chunk);
            acc
        }))
    }

    pub fn to_utf8(&self) -> Result<Cow<'_, str>, str::Utf8Error> {
        match self.to_bytes() {
            Cow::Borrowed(bytes) => str::from_utf8(bytes).map(Cow::Borrowed),
            Cow::Owned(bytes) => String::from_utf8(bytes)
                .map_err(|e| e.utf8_error())
                .map(Cow::Owned),
        }
    }
}
