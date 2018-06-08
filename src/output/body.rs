use bytes::Bytes;
use futures::Stream;
use hyper::body::{Body, Chunk};
use std::borrow::Cow;
use std::error::Error as StdError;

use input;

#[derive(Debug, Default)]
pub struct ResponseBody(Body);

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody(Body::empty())
    }
}

impl From<Body> for ResponseBody {
    fn from(body: Body) -> Self {
        ResponseBody(body)
    }
}

impl From<input::body::Chunk> for ResponseBody {
    fn from(chunk: input::body::Chunk) -> Self {
        ResponseBody(Body::from(chunk.0))
    }
}

macro_rules! impl_conversions {
    ($($t:ty,)*) => {$(
        impl From<$t> for ResponseBody {
            fn from(body: $t) -> Self {
                ResponseBody(body.into())
            }
        }
    )*};
}

impl_conversions![
    &'static str,
    &'static [u8],
    String,
    Vec<u8>,
    Cow<'static, str>,
    Cow<'static, [u8]>,
    Bytes,
    Chunk,
];

impl ResponseBody {
    pub fn empty() -> ResponseBody {
        Default::default()
    }

    pub fn wrap_stream<S>(stream: S) -> ResponseBody
    where
        S: Stream + Send + 'static,
        S::Error: Into<Box<StdError + Send + Sync + 'static>>,
        Chunk: From<S::Item>,
    {
        ResponseBody(Body::wrap_stream(stream))
    }

    pub(crate) fn into_hyp(self) -> Body {
        self.0
    }
}
