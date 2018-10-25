//! Components for constructing HTTP responses.

use bytes::{Buf, Bytes, IntoBuf};
use either::Either;
use futures::{Async, Future, Poll, Stream};
use http::header::{HeaderMap, HeaderValue};
use http::{header, Response, StatusCode};

use crate::error::{Error, HttpError, Never};
use crate::input::{self, Input};
use crate::server::service::http::{Body, Payload};

/// A type representing the message body in an HTTP response.
#[derive(Debug, Default)]
pub struct ResponseBody(Body);

impl ResponseBody {
    /// Creates an empty `ResponseBody`.
    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Wraps a `Stream` into a `ResponseBody`.
    pub fn wrap_stream<S>(stream: S) -> Self
    where
        S: Stream + Send + 'static,
        S::Error: Into<crate::server::CritError>,
        S::Item: IntoBuf,
    {
        ResponseBody(Body::wrap_stream(
            stream.map(|chunk| chunk.into_buf().collect::<Bytes>()),
        ))
    }
}

impl From<()> for ResponseBody {
    fn from(_: ()) -> Self {
        ResponseBody(Body::empty())
    }
}

macro_rules! impl_response_body {
    ($($t:ty,)*) => {$(
        impl From<$t> for ResponseBody {
            fn from(body: $t) -> Self {
                ResponseBody(Body::from(body))
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
    std::borrow::Cow<'static, str>,
    std::borrow::Cow<'static, [u8]>,
    crate::server::service::http::Body,
}

impl Payload for ResponseBody {
    type Data = <Body as Payload>::Data;
    type Error = <Body as Payload>::Error;

    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    fn poll_data(&mut self) -> Poll<Option<Self::Data>, Self::Error> {
        self.0.poll_data()
    }

    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    fn poll_trailers(&mut self) -> Poll<Option<HeaderMap>, Self::Error> {
        self.0.poll_trailers()
    }

    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    fn content_length(&self) -> Option<u64> {
        self.0.content_length()
    }
}

/// The type representing outputs returned from handlers.
pub type Output = ::http::Response<ResponseBody>;

/// A trait representing the conversion to an HTTP response.
pub trait Responder {
    /// The type of message body in the generated HTTP response.
    type Body: Into<ResponseBody>;

    /// The error type which will be returned from `respond_to`.
    type Error: Into<Error>;

    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error>;
}

impl<L, R> Responder for Either<L, R>
where
    L: Responder,
    R: Responder,
{
    type Body = ResponseBody;
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        match self {
            Either::Left(l) => l
                .respond_to(input)
                .map(|res| res.map(Into::into))
                .map_err(Into::into),
            Either::Right(r) => r
                .respond_to(input)
                .map(|res| res.map(Into::into))
                .map_err(Into::into),
        }
    }
}

impl Responder for () {
    type Body = ();
    type Error = Never;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let mut response = Response::new(());
        *response.status_mut() = StatusCode::NO_CONTENT;
        Ok(response)
    }
}

impl<T> Responder for Option<T>
where
    T: Responder,
{
    type Body = ResponseBody;
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self.ok_or_else(|| OptionError { _priv: () })?
            .respond_to(input)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
    }
}

#[doc(hidden)]
#[derive(Debug, failure::Fail)]
#[fail(display = "Not Found")]
pub struct OptionError {
    _priv: (),
}

impl HttpError for OptionError {
    fn status(&self) -> StatusCode {
        StatusCode::NOT_FOUND
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<T, E> Responder for Result<T, E>
where
    T: Responder,
    Error: From<E>,
{
    type Body = ResponseBody;
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self?
            .respond_to(input)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<T> Responder for Response<T>
where
    T: Into<ResponseBody>,
{
    type Body = T;
    type Error = Never;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self)
    }
}

impl Responder for &'static str {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(text_response(self))
    }
}

impl Responder for String {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(text_response(self))
    }
}

fn text_response<T>(body: T) -> Response<T> {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

/// The async variant of `Responder`.
pub trait AsyncResponder: Send + 'static + sealed::Sealed {
    /// The inner type of this responder.
    type Output: Responder;

    /// Polls for a result of inner `Responder`.
    // FIXME: replace the receiver type with PinMut<Self>
    fn poll_respond_to(&mut self, input: &mut Input<'_>) -> Poll<Output, Error>;
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<F> AsyncResponder for F
where
    F: Future + Send + 'static,
    F::Item: Responder,
    Error: From<F::Error>,
{
    type Output = F::Item;

    fn poll_respond_to(&mut self, input: &mut Input<'_>) -> Poll<Output, Error> {
        let x = futures::try_ready!(input::with_set_current(input, || Future::poll(self)));
        x.respond_to(input)
            .map(|res| Async::Ready(res.map(Into::into)))
            .map_err(Into::into)
    }
}

// TODO: switch bracket impls to std::future::Future
//
//   impl<F> AsyncResponder for F
//   where
//       F: Future + Send + 'static,
//       F::Output: Responder,
//   {
//       type Output = F::Output;
//
//       fn poll_respond_to(
//           self: PinMut<Self>,
//           cx: &mut Context,
//           input: &mut Input,
//       ) -> Poll<Result<Output, Error>> {
//           let x = ready!(input::with_set_current(input, || Future::poll(self, cx)));
//           Poll::Ready(x.respond_to(input))
//       }
//   }

mod sealed {
    use futures::Future;

    use super::Responder;
    use crate::error::Error;

    pub trait Sealed {}

    impl<F> Sealed for F
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {}
}
