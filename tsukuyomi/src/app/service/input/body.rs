//! Components for receiving incoming request bodies.

use bytes::{Bytes, BytesMut};
use futures::{Async, Future, Poll};
use std::mem;

use crate::server::service::http::Payload;
use crate::server::CritError;

#[doc(inline)]
pub use crate::server::service::http::{RequestBody, UpgradedIo};

// ==== ReadAll ====

/// A future to receive the entire of incoming message body.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct ReadAll {
    state: ReadAllState,
}

#[derive(Debug)]
enum ReadAllState {
    Init(Option<RequestBody>),
    Receiving(RequestBody, BytesMut),
    Done,
}

impl ReadAll {
    pub(super) fn init(body: Option<RequestBody>) -> Self {
        Self {
            state: ReadAllState::Init(body),
        }
    }
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
                    while let Some(chunk) = futures::try_ready!(body.poll_data()) {
                        buf.extend_from_slice(&*chunk);
                    }
                }
                Done => panic!("cannot resolve twice"),
            }

            match mem::replace(&mut self.state, Done) {
                Init(Some(body)) => {
                    self.state = Receiving(body, BytesMut::new());
                    continue;
                }
                Init(None) => return Err(failure::format_err!("").compat().into()),
                Receiving(_body, buf) => {
                    // debug_assert!(body.is_end_stream());
                    return Ok(Async::Ready(buf.freeze()));
                }
                Done => unreachable!(),
            }
        }
    }
}
