//! Components for receiving incoming request bodies.

use bytes::{Buf, Bytes, BytesMut};
use futures::{Async, Future, Poll, Stream};
use http::header::HeaderMap;
use hyper::body::{self, Body, Payload as _Payload};
use hyper::upgrade::OnUpgrade;
use hyperx::header::ContentType;
use mime;
use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Deref;
use std::{fmt, mem};

use error::{CritError, Error};
use future;
use input::Input;

// ==== RequestBody ====

/// A type representing a message body in the incoming HTTP request.
#[derive(Debug)]
pub struct RequestBody(Option<Body>);

impl RequestBody {
    pub(crate) fn from_hyp(body: Body) -> RequestBody {
        RequestBody(Some(body))
    }

    fn take_body(&mut self) -> Option<Body> {
        self.0.take()
    }

    /// Takes away the instance of raw message body if exists.
    pub fn forget(&mut self) {
        self.take_body().map(mem::drop);
    }

    /// Returns 'true' if the instance of raw message body has already taken away.
    pub fn is_gone(&self) -> bool {
        self.0.is_none()
    }

    /// Creates an instance of "Payload" from the raw message body.
    pub fn payload(&mut self) -> Payload {
        Payload(self.take_body())
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> ReadAll {
        ReadAll {
            state: ReadAllState::Init(self.take_body()),
        }
    }

    pub(crate) fn on_upgrade(&mut self) -> Option<OnUpgrade> {
        self.take_body().map(|body| body.on_upgrade())
    }
}

impl Default for RequestBody {
    fn default() -> Self {
        RequestBody(Some(Default::default()))
    }
}

impl From<()> for RequestBody {
    fn from(_: ()) -> Self {
        Default::default()
    }
}

macro_rules! impl_from_for_request_body {
    ($($t:ty,)*) => {$(
        impl From<$t> for RequestBody {
            fn from(body: $t) -> Self {
                RequestBody(Some(body.into()))
            }
        }
    )*};
}

impl_from_for_request_body![
    &'static str,
    &'static [u8],
    Vec<u8>,
    String,
    Cow<'static, str>,
    Cow<'static, [u8]>,
    Bytes,
];

// ==== Payload ====

/// A `Payload` representing the raw streaming body in an incoming HTTP request.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Payload(Option<Body>);

impl Payload {
    fn with_body<T>(&mut self, f: impl FnOnce(&mut Body) -> Result<T, CritError>) -> Result<T, CritError> {
        match self.0 {
            Some(ref mut bd) => f(bd),
            None => Err(format_err!("").compat().into()),
        }
    }
}

impl body::Payload for Payload {
    type Data = Chunk;
    type Error = CritError;

    fn poll_data(&mut self) -> Poll<Option<Chunk>, CritError> {
        self.with_body(|bd| {
            bd.poll_data()
                .map(|x| x.map(|c| c.map(Chunk::from_hyp)))
                .map_err(Into::into)
        })
    }

    fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, CritError> {
        self.with_body(|bd| bd.poll_trailers().map_err(Into::into))
    }

    fn is_end_stream(&self) -> bool {
        self.0.as_ref().map_or(true, |bd| bd.is_end_stream())
    }

    fn content_length(&self) -> Option<u64> {
        self.0.as_ref().map_or(None, |bd| bd.content_length())
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

/// A future to receive the entire of incoming message body.
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

impl ReadAll {
    /// Attempts to receive the entire of message data.
    pub fn poll_ready(&mut self) -> future::Poll<Result<Bytes, CritError>> {
        use self::ReadAllState::*;
        loop {
            match self.state {
                Init(..) => {}
                Receiving(ref mut body, ref mut buf) => {
                    while let Some(chunk) = try_ready_compat!(body.poll_data()) {
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
                Init(None) => return future::Poll::Ready(Err(format_err!("").compat().into())),
                Receiving(_body, buf) => {
                    // debug_assert!(body.is_end_stream());
                    return future::Poll::Ready(Ok(buf.freeze()));
                }
                Done => unreachable!(),
            }
        }
    }

    /// Creates a future from `self` that will convert the received data into a value of `T`.
    pub fn convert_to<T>(self) -> ConvertTo<T>
    where
        T: FromData + 'static,
    {
        ConvertTo {
            read_all: self,
            _marker: PhantomData,
        }
    }
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.poll_ready().into()
    }
}

// ==== ConvertTo ====

/// A future to receive the entire of message body and then convert the data into a value of `T`.
#[must_use = "futures do nothing unless polled"]
pub struct ConvertTo<T> {
    read_all: ReadAll,
    _marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for ConvertTo<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ConvertTo").field("read_all", &self.read_all).finish()
    }
}

impl<T> ConvertTo<T>
where
    T: FromData,
{
    /// Attempts to convert the incoming message data into an value of `T`.
    pub fn poll_ready(&mut self, input: &Input) -> future::Poll<Result<T, Error>> {
        let data = try_ready_compat!(self.read_all.poll().map_err(Error::critical));
        future::Poll::Ready(T::from_data(data, input))
    }
}

impl<T> Future for ConvertTo<T>
where
    T: FromData,
{
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let data = try_ready!(self.read_all.poll().map_err(Error::critical));
        Input::with_current(|input| T::from_data(data, input)).map(Async::Ready)
    }
}

/// A trait representing the conversion to certain type.
pub trait FromData: Sized {
    /// Perform conversion from a received buffer of bytes into a value of `Self`.
    fn from_data(data: Bytes, input: &Input) -> Result<Self, Error>;
}

impl FromData for String {
    fn from_data(data: Bytes, input: &Input) -> Result<Self, Error> {
        if let Some(ContentType(m)) = input.header()? {
            if m != mime::TEXT_PLAIN {
                return Err(Error::bad_request(format_err!("the content type must be text/plain")));
            }
            if m.get_param("charset").map_or(true, |charset| charset != "utf-8") {
                return Err(Error::bad_request(format_err!("the charset must be utf-8")));
            }
        }

        String::from_utf8(data.to_vec()).map_err(Error::bad_request)
    }
}
