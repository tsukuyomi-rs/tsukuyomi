use {
    bytes::{Buf, Bytes, IntoBuf},
    either::Either,
    futures01::{Poll, Stream},
    izanami_http::buf_stream::{BufStream, SizeHint},
    std::{fmt, io},
};

pub type Chunk = Box<dyn Buf + Send + 'static>;
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

trait BoxedBufStream {
    fn poll_buf_boxed(&mut self) -> Poll<Option<Chunk>, Error>;
    fn size_hint_boxed(&self) -> SizeHint;
    fn consume_hint_boxed(&mut self, amt: usize);
}

impl<T> BoxedBufStream for T
where
    T: BufStream,
    T::Item: Send + 'static,
    T::Error: Into<Error>,
{
    fn poll_buf_boxed(&mut self) -> Poll<Option<Chunk>, Error> {
        self.poll_buf()
            .map(|x| x.map(|opt| opt.map(|chunk| Box::new(chunk) as Chunk)))
            .map_err(Into::into)
    }

    fn size_hint_boxed(&self) -> SizeHint {
        self.size_hint()
    }

    fn consume_hint_boxed(&mut self, amt: usize) {
        self.consume_hint(amt)
    }
}

/// A type representing the message body in an HTTP response.
#[derive(Debug)]
pub struct ResponseBody(Inner);

enum Inner {
    Sized(Bytes),
    Chunked(Box<dyn BoxedBufStream + Send + 'static>),
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
        ResponseBody(Inner::Sized(Bytes::new()))
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
            fn poll_buf_boxed(&mut self) -> Poll<Option<Chunk>, Error> {
                self.0
                    .poll()
                    .map(|x| x.map(|opt| opt.map(|buf| Box::new(buf.into_buf()) as Chunk)))
                    .map_err(Into::into)
            }
            fn size_hint_boxed(&self) -> SizeHint {
                SizeHint::new()
            }
            fn consume_hint_boxed(&mut self, _: usize) {}
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
                ResponseBody(Inner::Sized(body.into()))
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

impl BufStream for ResponseBody {
    type Item = Either<io::Cursor<Bytes>, Chunk>;
    type Error = Error;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match &mut self.0 {
            Inner::Sized(chunk) => chunk
                .poll_buf()
                .map(|x| x.map(|opt| opt.map(|chunk| Either::Left(chunk))))
                .map_err(Into::into),
            Inner::Chunked(chunks) => chunks
                .poll_buf_boxed()
                .map(|x| x.map(|opt| opt.map(Either::Right))),
        }
    }
}
