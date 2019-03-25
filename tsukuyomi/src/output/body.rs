use {
    bytes::{Buf, Bytes, IntoBuf},
    futures01::{Async, Poll, Stream},
    izanami::http::HttpBody,
    std::fmt,
    tokio_buf::SizeHint,
};

// ===== Data =====

/// A chunk of bytes produced by `ResponseBody`.
#[derive(Debug)]
pub struct Data(DataInner);

enum DataInner {
    Bytes(Bytes),
    Boxed(Box<dyn Buf + Send + 'static>),
}

impl fmt::Debug for DataInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataInner::Bytes(..) => f.debug_struct("Bytes").finish(),
            DataInner::Boxed(..) => f.debug_struct("Boxed").finish(),
        }
    }
}

impl Buf for Data {
    fn remaining(&self) -> usize {
        match self.0 {
            DataInner::Bytes(ref data) => data.len(),
            DataInner::Boxed(ref data) => data.remaining(),
        }
    }

    fn bytes(&self) -> &[u8] {
        match self.0 {
            DataInner::Bytes(ref data) => data.as_ref(),
            DataInner::Boxed(ref data) => data.bytes(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self.0 {
            DataInner::Bytes(ref mut data) => data.advance(cnt),
            DataInner::Boxed(ref mut data) => data.advance(cnt),
        }
    }
}

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// ===== ResponseBody =====

trait BoxedBufStream: Send + 'static {
    fn poll_buf(&mut self) -> Poll<Option<Data>, Error>;
    fn size_hint(&self) -> SizeHint;
}

/// A type representing the message body in an HTTP response.
#[derive(Debug)]
pub struct ResponseBody(Inner);

enum Inner {
    Empty,
    Sized(Option<Bytes>),
    Chunked(Box<dyn BoxedBufStream>),
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inner::Empty => f.debug_struct("Empty").finish(),
            Inner::Sized(..) => f.debug_struct("Sized").finish(),
            Inner::Chunked(..) => f.debug_struct("Chunked").finish(),
        }
    }
}

impl Default for ResponseBody {
    fn default() -> Self {
        Self::empty()
    }
}

impl ResponseBody {
    /// Creates an empty `ResponseBody`.
    #[inline]
    pub fn empty() -> Self {
        ResponseBody(Inner::Empty)
    }

    /// Wraps a `Stream` into a `ResponseBody`.
    pub fn wrap_stream<S>(stream: S) -> Self
    where
        S: Stream + Send + 'static,
        S::Item: IntoBuf,
        <S::Item as IntoBuf>::Buf: Send + 'static,
        S::Error: Into<Error>,
    {
        struct WrapStream<S>(S);

        impl<S> BoxedBufStream for WrapStream<S>
        where
            S: Stream + Send + 'static,
            S::Item: IntoBuf,
            <S::Item as IntoBuf>::Buf: Send + 'static,
            S::Error: Into<Error>,
        {
            fn poll_buf(&mut self) -> Poll<Option<Data>, Error> {
                self.0
                    .poll()
                    .map(|x| {
                        x.map(|opt| opt.map(|buf| Data(DataInner::Boxed(Box::new(buf.into_buf())))))
                    })
                    .map_err(Into::into)
            }

            fn size_hint(&self) -> SizeHint {
                SizeHint::new()
            }
        }

        ResponseBody(Inner::Chunked(Box::new(WrapStream(stream))))
    }
}

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody::empty()
    }
}

macro_rules! impl_response_body {
    ($($t:ty,)*) => {$(
        impl From<$t> for ResponseBody {
            fn from(body: $t) -> Self {
                ResponseBody(Inner::Sized(Some(body.into())))
            }
        }
    )*};
}

impl_response_body! {
    &'static str,
    &'static [u8],
    String,
    Vec<u8>,
    bytes::Bytes,
    //std::borrow::Cow<'static, str>,
    //std::borrow::Cow<'static, [u8]>,
}

impl HttpBody for ResponseBody {
    type Data = Data;
    type Error = Error;

    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        match self.0 {
            Inner::Empty => Ok(Async::Ready(None)),
            Inner::Sized(ref mut chunk) => {
                let res = chunk.take().map(|data| Data(DataInner::Bytes(data)));
                Ok(Async::Ready(res))
            }
            Inner::Chunked(ref mut chunks) => chunks.poll_buf(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        let mut hint = SizeHint::new();
        match self.0 {
            Inner::Empty => hint.set_upper(0),
            Inner::Sized(Some(ref data)) => hint.set_upper(data.len() as u64),
            Inner::Sized(..) => (),
            Inner::Chunked(..) => (),
        }
        hint
    }

    fn is_end_stream(&self) -> bool {
        match self.0 {
            Inner::Empty => true,
            Inner::Sized(ref data) => data.as_ref().map_or(true, |data| data.is_empty()),
            Inner::Chunked(..) => false,
        }
    }

    fn content_length(&self) -> Option<u64> {
        match self.0 {
            Inner::Empty => None,
            Inner::Sized(Some(ref data)) => Some(data.len() as u64),
            _ => None,
        }
    }
}
