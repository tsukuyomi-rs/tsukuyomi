use bytes::Bytes;
use futures::Stream;
use hyper::body::{Body, Chunk};
use std::borrow::Cow;
use std::error::Error as StdError;
use std::{mem, str};

use error::CritError;
use future::Poll;
use input;

/// A type representing the message body in HTTP response.
#[derive(Debug)]
pub struct ResponseBody(ResponseBodyInner);

#[derive(Debug)]
enum ResponseBodyInner {
    Empty,
    Sized(Bytes),
    Chunked(Body),
}

use self::ResponseBodyInner as Inner;

impl Default for ResponseBody {
    fn default() -> Self {
        ResponseBody(Inner::Empty)
    }
}

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody(Inner::Empty)
    }
}

impl From<Body> for ResponseBody {
    fn from(body: Body) -> Self {
        ResponseBody(Inner::Chunked(body))
    }
}

impl From<input::body::Chunk> for ResponseBody {
    fn from(chunk: input::body::Chunk) -> Self {
        ResponseBody(Inner::Sized(chunk.0.into_bytes()))
    }
}

impl From<Chunk> for ResponseBody {
    fn from(chunk: Chunk) -> Self {
        ResponseBody(Inner::Sized(chunk.into_bytes()))
    }
}

macro_rules! impl_conversions {
    ($($t:ty,)*) => {$(
        impl From<$t> for ResponseBody {
            fn from(body: $t) -> Self {
                ResponseBody(Inner::Sized(body.into()))
            }
        }
    )*};
}

impl_conversions![&'static str, &'static [u8], String, Vec<u8>, Bytes,];

impl From<Cow<'static, str>> for ResponseBody {
    fn from(body: Cow<'static, str>) -> Self {
        match body {
            Cow::Borrowed(bytes) => bytes.into(),
            Cow::Owned(bytes) => bytes.into(),
        }
    }
}

impl From<Cow<'static, [u8]>> for ResponseBody {
    fn from(body: Cow<'static, [u8]>) -> Self {
        match body {
            Cow::Borrowed(bytes) => bytes.into(),
            Cow::Owned(bytes) => bytes.into(),
        }
    }
}

impl ResponseBody {
    /// Creates an empty `ResponseBody`.
    pub fn empty() -> ResponseBody {
        Default::default()
    }

    /// Wraps a stream of buffers of bytes and creates a chunked `ResponseBody`.
    pub fn wrap_stream<S>(stream: S) -> ResponseBody
    where
        S: Stream + Send + 'static,
        S::Error: Into<Box<StdError + Send + Sync + 'static>>,
        Chunk: From<S::Item>,
    {
        ResponseBody(Inner::Chunked(Body::wrap_stream(stream)))
    }

    pub(crate) fn content_length(&self) -> Option<usize> {
        match self.0 {
            Inner::Empty => Some(0),
            Inner::Sized(ref bytes) => Some(bytes.len()),
            _ => None,
        }
    }

    pub(crate) fn into_hyp(self) -> Body {
        match self.0 {
            Inner::Empty => Body::empty(),
            Inner::Sized(bytes) => Body::from(bytes),
            Inner::Chunked(body) => body,
        }
    }

    pub(crate) fn receive(self) -> Receive {
        match self.0 {
            Inner::Empty => Receive(ReceiveInner::Empty),
            Inner::Sized(data) => Receive(ReceiveInner::Sized(Some(data))),
            Inner::Chunked(body) => Receive(ReceiveInner::Chunked(body, vec![])),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Receive(ReceiveInner);

#[derive(Debug)]
enum ReceiveInner {
    Empty,
    Sized(Option<Bytes>),
    Chunked(Body, Vec<Bytes>),
}

impl Receive {
    pub(crate) fn poll_ready(&mut self) -> Poll<Result<Data, CritError>> {
        match self.0 {
            ReceiveInner::Empty => Poll::Ready(Ok(Data(DataInner::Empty))),
            ReceiveInner::Sized(ref mut data) => Poll::Ready(Ok(Data(DataInner::Sized(
                data.take().expect("The response body has already resolved").into(),
            )))),
            ReceiveInner::Chunked(ref mut body, ref mut chunks) => loop {
                match ready!(body.poll().into()) {
                    Ok(Some(chunk)) => {
                        chunks.push(chunk.into());
                    }
                    Ok(None) => {
                        let chunks = mem::replace(chunks, vec![]);
                        return Poll::Ready(Ok(Data(DataInner::Chunked(chunks))));
                    }
                    Err(err) => return Poll::Ready(Err(err.into())),
                }
            },
        }
    }
}

/// A type representing a received HTTP message data from the server.
///
/// This type is usually used by the testing framework.
#[derive(Debug)]
pub struct Data(DataInner);

#[derive(Debug)]
enum DataInner {
    Empty,
    Sized(Bytes),
    Chunked(Vec<Bytes>),
}

#[allow(missing_docs)]
impl Data {
    pub fn is_sized(&self) -> bool {
        match self.0 {
            DataInner::Empty | DataInner::Sized(..) => true,
            _ => false,
        }
    }

    pub fn is_chunked(&self) -> bool {
        !self.is_sized()
    }

    pub fn len(&self) -> Option<usize> {
        match self.0 {
            DataInner::Empty => Some(0),
            DataInner::Sized(ref data) => Some(data.len()),
            _ => None,
        }
    }

    pub fn as_chunks(&self) -> Option<&[Bytes]> {
        match self.0 {
            DataInner::Chunked(ref chunks) => Some(&chunks[..]),
            _ => None,
        }
    }

    pub fn to_bytes(&self) -> Cow<[u8]> {
        match self.0 {
            DataInner::Empty => Cow::Borrowed(&[]),
            DataInner::Sized(ref data) => Cow::Borrowed(&data[..]),
            DataInner::Chunked(ref chunks) => Cow::Owned(chunks.iter().fold(Vec::new(), |mut acc, chunk| {
                acc.extend_from_slice(&*chunk);
                acc
            })),
        }
    }

    pub fn to_utf8(&self) -> Result<Cow<str>, str::Utf8Error> {
        match self.to_bytes() {
            Cow::Borrowed(bytes) => str::from_utf8(bytes).map(Cow::Borrowed),
            Cow::Owned(bytes) => String::from_utf8(bytes).map_err(|e| e.utf8_error()).map(Cow::Owned),
        }
    }
}
