use bytes::Bytes;
use futures::Stream;
use http::Response;
use hyper::body::{Body, Chunk};
use std::borrow::Cow;
use std::error::Error as StdError;

use request::RequestBody;

#[derive(Debug)]
pub struct Output(Response<ResponseBody>);

impl<T> From<Response<T>> for Output
where
    T: Into<ResponseBody>,
{
    fn from(response: Response<T>) -> Self {
        Output(response.map(Into::into))
    }
}

impl Output {
    pub(crate) fn deconstruct(self) -> Response<Body> {
        self.0.map(ResponseBody::into_hyp)
    }
}

#[derive(Debug, Default)]
pub struct ResponseBody(Body);

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody(Body::empty())
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
];

impl From<RequestBody> for ResponseBody {
    fn from(body: RequestBody) -> ResponseBody {
        ResponseBody(body.into_hyp())
    }
}

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
