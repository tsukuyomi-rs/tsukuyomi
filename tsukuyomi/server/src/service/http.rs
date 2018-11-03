pub use self::imp::{HttpRequest, HttpResponse, OnUpgrade, RequestBody, UpgradedIo};

#[doc(no_inline)]
pub use hyper::body::{Body, Payload};

pub(crate) mod imp {
    use std::io;

    use bytes::{Buf, BufMut};
    use futures::{Future, Poll, Stream};
    use http;
    use http::header::HeaderMap;
    use hyper;
    use hyper::body::Payload;
    use tokio;

    use CritError;

    // ==== HttpRequest ====

    pub trait HttpRequest: HttpRequestImpl {}

    impl<T> HttpRequest for http::Request<T> where T: From<RequestBody> {}

    pub trait HttpRequestImpl {
        fn from_request(request: http::Request<RequestBody>) -> Self;
    }

    impl<T> HttpRequestImpl for http::Request<T>
    where
        T: From<RequestBody>,
    {
        fn from_request(request: http::Request<RequestBody>) -> Self {
            request.map(Into::into)
        }
    }

    #[derive(Debug)]
    pub struct OnUpgrade(hyper::upgrade::OnUpgrade);

    impl Future for OnUpgrade {
        type Item = UpgradedIo;
        type Error = CritError;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            self.0.poll().map(|x| x.map(UpgradedIo)).map_err(Into::into)
        }
    }

    /// An asynchronous I/O upgraded from HTTP connection.
    ///
    /// Currenly, this type is implemented as a thin wrapper of `hyper::upgrade::Upgraded`.
    #[derive(Debug)]
    pub struct UpgradedIo(hyper::upgrade::Upgraded);

    impl io::Read for UpgradedIo {
        #[inline]
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl io::Write for UpgradedIo {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }

        #[inline]
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl tokio::io::AsyncRead for UpgradedIo {
        #[inline]
        unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
            tokio::io::AsyncRead::prepare_uninitialized_buffer(&self.0, buf)
        }

        #[inline]
        fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
            tokio::io::AsyncRead::read_buf(&mut self.0, buf)
        }
    }

    impl tokio::io::AsyncWrite for UpgradedIo {
        #[inline]
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            tokio::io::AsyncWrite::shutdown(&mut self.0)
        }

        #[inline]
        fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
            tokio::io::AsyncWrite::write_buf(&mut self.0, buf)
        }
    }

    #[derive(Debug)]
    pub struct RequestBody(pub(crate) hyper::body::Body);

    impl RequestBody {
        #[inline]
        pub fn on_upgrade(self) -> OnUpgrade {
            OnUpgrade(self.0.on_upgrade())
        }
    }

    impl Payload for RequestBody {
        type Data = hyper::body::Chunk;
        type Error = hyper::Error;

        #[inline]
        fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
            self.0.poll_data()
        }

        #[inline]
        fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
            self.0.poll_trailers()
        }

        #[inline]
        fn is_end_stream(&self) -> bool {
            self.0.is_end_stream()
        }

        #[inline]
        fn content_length(&self) -> Option<u64> {
            self.0.content_length()
        }
    }

    impl Stream for RequestBody {
        type Item = hyper::body::Chunk;
        type Error = hyper::Error;

        #[inline]
        fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
            self.0.poll()
        }
    }

    // ==== HttpResponse ====

    pub trait HttpResponse: HttpResponseImpl {}

    impl<T> HttpResponse for http::Response<T> where T: Payload {}

    pub trait HttpResponseImpl {
        type Body: Payload;
        fn into_response(self) -> http::Response<Self::Body>;
    }

    impl<T> HttpResponseImpl for http::Response<T>
    where
        T: Payload,
    {
        type Body = T;

        fn into_response(self) -> http::Response<Self::Body> {
            self
        }
    }
}
