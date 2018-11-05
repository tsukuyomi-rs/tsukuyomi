//! Components for receiving incoming request bodies.

use bytes::{Bytes, BytesMut};
use futures::{Async, Future, IntoFuture, Poll};
use std::mem;

use crate::server::rt;
use crate::server::server::CritError;
use crate::server::service::http::{Payload as _Payload, RequestBody as RawBody, UpgradedIo};

// ==== RequestBody ====

/// A type representing a message body in the incoming HTTP request.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct RequestBody {
    body: Option<RawBody>,
    is_upgraded: bool,
}

impl From<RawBody> for RequestBody {
    fn from(body: RawBody) -> Self {
        Self {
            body: Some(body),
            is_upgraded: false,
        }
    }
}

impl RequestBody {
    /// Returns 'true' if the instance of raw message body has already taken away.
    pub fn is_gone(&self) -> bool {
        self.body.is_none()
    }

    /// Creates an instance of "Payload" from the raw message body.
    pub fn raw(&mut self) -> Option<RawBody> {
        self.body.take()
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> ReadAll {
        ReadAll {
            state: ReadAllState::Init(self.body.take()),
        }
    }

    /// Returns 'true' if the upgrade function is set.
    pub fn is_upgraded(&self) -> bool {
        self.is_upgraded
    }

    /// Registers the upgrade function to this request.
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        if self.is_upgraded {
            return Err(on_upgrade);
        }
        self.is_upgraded = true;

        let body = self.body.take().expect("The body has already gone");
        rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
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
    Init(Option<RawBody>),
    Receiving(RawBody, BytesMut),
    Done,
}

impl ReadAll {
    /// Attempts to receive the entire of message data.
    pub fn poll_ready(&mut self) -> Poll<Bytes, CritError> {
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

impl Future for ReadAll {
    type Item = Bytes;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.poll_ready()
    }
}
