//! Components for receiving incoming request bodies.

use {
    super::localmap::{local_key, LocalData},
    crate::error::HttpError,
    bytes::{Buf, Bytes, BytesMut},
    futures01::{Async, Future, Poll},
    http::StatusCode,
    izanami::{
        h1::{Data as H1Data, RequestBody as H1Body},
        h2::{Data as H2Data, RequestBody as H2Body},
        http::body::HttpBody,
    },
    std::fmt,
};

#[derive(Debug)]
pub struct Chunk(ChunkInner);

#[derive(Debug)]
#[allow(dead_code)]
enum ChunkInner {
    H1(H1Data),
    H2(H2Data),
    Raw(Bytes),
}

impl AsRef<[u8]> for Chunk {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl Buf for Chunk {
    fn remaining(&self) -> usize {
        match self.0 {
            ChunkInner::H1(ref data) => data.remaining(),
            ChunkInner::H2(ref data) => data.remaining(),
            ChunkInner::Raw(ref data) => data.len(),
        }
    }

    fn bytes(&self) -> &[u8] {
        match self.0 {
            ChunkInner::H1(ref data) => data.bytes(),
            ChunkInner::H2(ref data) => data.bytes(),
            ChunkInner::Raw(ref data) => data.as_ref(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self.0 {
            ChunkInner::H1(ref mut data) => data.advance(cnt),
            ChunkInner::H2(ref mut data) => data.advance(cnt),
            ChunkInner::Raw(ref mut data) => data.advance(cnt),
        }
    }
}

pub struct Error(Box<dyn std::error::Error + Send + Sync + 'static>);

impl std::ops::Deref for Error {
    type Target = dyn std::error::Error + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl HttpError for Error {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct RequestBody(RequestBodyInner);

#[derive(Debug)]
enum RequestBodyInner {
    H1(H1Body),
    H2(H2Body),
    Raw(Option<Bytes>),
}

impl RequestBody {
    pub fn new(data: impl Into<Bytes>) -> Self {
        RequestBody(RequestBodyInner::Raw(Some(data.into())))
    }
}

impl From<H1Body> for RequestBody {
    fn from(body: H1Body) -> Self {
        RequestBody(RequestBodyInner::H1(body))
    }
}

impl From<H2Body> for RequestBody {
    fn from(body: H2Body) -> Self {
        RequestBody(RequestBodyInner::H2(body))
    }
}

impl LocalData for RequestBody {
    local_key! {
        /// The local key to manage the request body
        /// stored in the current context.
        const KEY: Self;
    }
}

impl HttpBody for RequestBody {
    type Data = Chunk;
    type Error = Error;

    #[inline]
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        match self.0 {
            RequestBodyInner::H1(ref mut body) => body
                .poll_data()
                .map(|x| x.map(|opt| opt.map(|data| Chunk(ChunkInner::H1(data)))))
                .map_err(Error),
            RequestBodyInner::H2(ref mut body) => body
                .poll_data()
                .map(|x| x.map(|opt| opt.map(|data| Chunk(ChunkInner::H2(data)))))
                .map_err(Error),
            RequestBodyInner::Raw(ref mut data) => Ok(Async::Ready(
                data.take().map(|data| Chunk(ChunkInner::Raw(data))),
            )),
        }
    }
}

impl RequestBody {
    pub fn read_all(self) -> ReadAll {
        ReadAll {
            body: self,
            acc: BytesMut::new(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct ReadAll {
    body: RequestBody,
    acc: BytesMut,
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Some(buf) = futures01::try_ready!(self.body.poll_data()) {
            self.acc.extend_from_slice(buf.bytes());
        }

        let buf = std::mem::replace(&mut self.acc, BytesMut::new()).freeze();
        Ok(buf.into())
    }
}
