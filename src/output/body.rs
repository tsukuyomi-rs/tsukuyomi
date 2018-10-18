use bytes::Bytes;
use futures::Stream;
use hyper::body::{Body, Chunk};
use std::borrow::Cow;
use std::error::Error as StdError;
use std::str;

use crate::input;

/// A type representing the message body in HTTP response.
#[derive(Debug)]
pub struct ResponseBody(pub(crate) ResponseBodyKind);

#[derive(Debug)]
pub(crate) enum ResponseBodyKind {
    Empty,
    Sized(Bytes),
    Chunked(Body),
}

impl Default for ResponseBody {
    fn default() -> Self {
        ResponseBody(ResponseBodyKind::Empty)
    }
}

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody(ResponseBodyKind::Empty)
    }
}

impl From<Body> for ResponseBody {
    fn from(body: Body) -> Self {
        ResponseBody(ResponseBodyKind::Chunked(body))
    }
}

impl From<input::body::Chunk> for ResponseBody {
    fn from(chunk: input::body::Chunk) -> Self {
        ResponseBody(ResponseBodyKind::Sized(chunk.0.into_bytes()))
    }
}

impl From<Chunk> for ResponseBody {
    fn from(chunk: Chunk) -> Self {
        ResponseBody(ResponseBodyKind::Sized(chunk.into_bytes()))
    }
}

macro_rules! impl_conversions {
    ($($t:ty,)*) => {$(
        impl From<$t> for ResponseBody {
            fn from(body: $t) -> Self {
                ResponseBody(ResponseBodyKind::Sized(body.into()))
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
        S::Error: Into<Box<dyn StdError + Send + Sync + 'static>>,
        Chunk: From<S::Item>,
    {
        ResponseBody(ResponseBodyKind::Chunked(Body::wrap_stream(stream)))
    }

    pub(crate) fn content_length(&self) -> Option<usize> {
        match self.0 {
            ResponseBodyKind::Empty => Some(0),
            ResponseBodyKind::Sized(ref bytes) => Some(bytes.len()),
            _ => None,
        }
    }

    pub(crate) fn into_hyp(self) -> Body {
        match self.0 {
            ResponseBodyKind::Empty => Body::empty(),
            ResponseBodyKind::Sized(bytes) => Body::from(bytes),
            ResponseBodyKind::Chunked(body) => body,
        }
    }
}
