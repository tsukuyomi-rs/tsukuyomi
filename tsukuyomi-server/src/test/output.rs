use {
    bytes::{Buf, Bytes},
    futures::{Async, Poll},
    http::header::HeaderMap,
    hyper::body::Payload,
    std::{borrow::Cow, mem, str},
};

#[derive(Debug)]
pub(super) struct Receive<Bd: Payload> {
    state: ReceiveState<Bd>,
    content_length: Option<u64>,
}

#[derive(Debug)]
enum ReceiveState<Bd: Payload> {
    Init(Bd),
    InFlight {
        body: Bd,
        chunks: Vec<Bytes>,
        end_of_chunks: bool,
    },
    Ready(Output),
    Gone,
}

impl<Bd: Payload> Receive<Bd> {
    pub(super) fn new(body: Bd) -> Self {
        let content_length = body.content_length();
        Self {
            state: ReceiveState::Init(body),
            content_length,
        }
    }

    pub(super) fn poll_ready(&mut self) -> Poll<(), Bd::Error> {
        loop {
            let trailers = match self.state {
                ReceiveState::Init(..) => None,
                ReceiveState::InFlight {
                    ref mut body,
                    ref mut chunks,
                    ref mut end_of_chunks,
                } => {
                    if *end_of_chunks {
                        futures::try_ready!(body.poll_trailers())
                    } else {
                        while let Some(chunk) = futures::try_ready!(body.poll_data()) {
                            chunks.push(chunk.collect());
                        }
                        *end_of_chunks = true;
                        continue;
                    }
                }
                ReceiveState::Ready(..) => return Ok(Async::Ready(())),
                ReceiveState::Gone => panic!("The future has already polled"),
            };

            match mem::replace(&mut self.state, ReceiveState::Gone) {
                ReceiveState::Init(body) => {
                    self.state = ReceiveState::InFlight {
                        body,
                        chunks: vec![],
                        end_of_chunks: false,
                    };
                }
                ReceiveState::InFlight {
                    chunks,
                    end_of_chunks,
                    ..
                } => {
                    debug_assert!(end_of_chunks);
                    self.state = ReceiveState::Ready(Output {
                        chunks,
                        trailers,
                        content_length: self.content_length,
                    });
                    return Ok(Async::Ready(()));
                }
                ReceiveState::Ready(..) | ReceiveState::Gone => {
                    unreachable!("unexpected condition")
                }
            }
        }
    }

    pub(super) fn into_data(self) -> Option<Output> {
        match self.state {
            ReceiveState::Ready(data) => Some(data),
            _ => None,
        }
    }
}

/// A type representing a received HTTP message data from the server.
#[derive(Debug)]
pub struct Output {
    chunks: Vec<Bytes>,
    trailers: Option<HeaderMap>,
    content_length: Option<u64>,
}

#[allow(missing_docs)]
impl Output {
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
