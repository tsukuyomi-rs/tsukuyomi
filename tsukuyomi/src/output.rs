//! Components for constructing HTTP responses.

use bytes::{Buf, Bytes, IntoBuf};
use either::Either;
use futures::{Poll, Stream};
use http::header::HeaderMap;
use http::{Response, StatusCode};
use hyper::body::{Body, Payload};
use serde::Serialize;

use crate::error::{Error, Never};
use crate::input::body::RequestBody;
use crate::input::Input;
use crate::server::CritError;

pub use tsukuyomi_macros::Responder;

// not a public API.
#[doc(hidden)]
pub mod internal {
    use crate::error::Error;
    use crate::input::Input;
    use crate::output::{Responder, ResponseBody};

    pub use http::Response;

    #[inline]
    pub fn respond_to<T>(t: T, input: &mut Input<'_>) -> Result<Response<ResponseBody>, Error>
    where
        T: Responder,
    {
        Responder::respond_to(t, input)
            .map(|resp| resp.map(Into::into))
            .map_err(Into::into)
    }
}

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
        S::Error: Into<CritError>,
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

impl From<RequestBody> for ResponseBody {
    fn from(body: RequestBody) -> Self {
        ResponseBody(body.into_inner())
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
    hyper::Body,
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
        self.ok_or_else(|| crate::error::not_found("None"))?
            .respond_to(input)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
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
    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::responder::plain(self, input)
    }
}

impl Responder for String {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::responder::plain(self, input)
    }
}

impl Responder for serde_json::Value {
    type Body = String;
    type Error = Never;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self::responder::make_response(
            self.to_string(),
            "application/json",
        ))
    }
}

/// Creates an instance of `Responder` from the specified function.
pub fn responder<F, T, E>(f: F) -> impl Responder
where
    F: FnOnce(&mut Input<'_>) -> Result<Response<T>, E>,
    T: Into<ResponseBody>,
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    #[cfg_attr(feature = "cargo-clippy", allow(stutter))]
    pub struct ResponderFn<F>(F);

    impl<F, T, E> Responder for ResponderFn<F>
    where
        F: FnOnce(&mut Input<'_>) -> Result<Response<T>, E>,
        T: Into<ResponseBody>,
        E: Into<Error>,
    {
        type Body = T;
        type Error = E;

        #[inline]
        fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
            (self.0)(input)
        }
    }

    ResponderFn(f)
}

/// Creates a JSON responder from the specified data.
#[inline]
pub fn json<T>(data: T) -> impl Responder
where
    T: Serialize,
{
    self::responder(move |input| self::responder::json(data, input))
}

/// Creates a JSON responder with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> impl Responder
where
    T: Serialize,
{
    self::responder(move |input| self::responder::json_pretty(data, input))
}

/// Creates an HTML responder with the specified response body.
#[inline]
pub fn html<T>(body: T) -> impl Responder
where
    T: Into<ResponseBody>,
{
    self::responder(move |input| self::responder::html(body, input))
}

#[allow(missing_docs)]
pub mod redirect {
    use super::*;

    use http::{Response, StatusCode};
    use std::borrow::Cow;

    #[derive(Debug)]
    pub struct Redirect {
        status: StatusCode,
        location: Cow<'static, str>,
    }

    impl Redirect {
        pub fn new<T>(status: StatusCode, location: T) -> Self
        where
            T: Into<Cow<'static, str>>,
        {
            Self {
                status,
                location: location.into(),
            }
        }
    }

    impl Responder for Redirect {
        type Body = ();
        type Error = Never;

        #[inline]
        fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
            Ok(Response::builder()
                .status(self.status)
                .header("location", &*self.location)
                .body(())
                .expect("should be a valid response"))
        }
    }

    macro_rules! define_funcs {
        ($( $name:ident => $STATUS:ident, )*) => {$(
            #[inline]
            pub fn $name<T>(location: T) -> Redirect
            where
                T: Into<Cow<'static, str>>,
            {
                Redirect::new(StatusCode::$STATUS, location)
            }
        )*};
    }

    define_funcs! {
        moved_permanently => MOVED_PERMANENTLY,
        found => FOUND,
        see_other => SEE_OTHER,
        temporary_redirect => TEMPORARY_REDIRECT,
        permanent_redirect => PERMANENT_REDIRECT,
        to => MOVED_PERMANENTLY,
    }
}

#[allow(missing_docs)]
pub mod responder {
    use http::Response;
    use serde::Serialize;

    use super::ResponseBody;
    use crate::error::{Error, Never};
    use crate::input::Input;

    #[inline]
    pub fn json<T>(data: T, _: &mut Input<'_>) -> Result<Response<Vec<u8>>, Error>
    where
        T: Serialize,
    {
        serde_json::to_vec(&data)
            .map(|body| self::make_response(body, "application/json"))
            .map_err(crate::error::internal_server_error)
    }

    #[inline]
    pub fn json_pretty<T>(data: T, _: &mut Input<'_>) -> Result<Response<Vec<u8>>, Error>
    where
        T: Serialize,
    {
        serde_json::to_vec_pretty(&data)
            .map(|body| self::make_response(body, "application/json"))
            .map_err(crate::error::internal_server_error)
    }

    #[inline]
    pub fn html<T>(body: T, _: &mut Input<'_>) -> Result<Response<T>, Never>
    where
        T: Into<ResponseBody>,
    {
        Ok(self::make_response(body, "text/html"))
    }

    #[inline]
    pub fn plain<T>(body: T, _: &mut Input<'_>) -> Result<Response<T>, Never>
    where
        T: Into<ResponseBody>,
    {
        Ok(self::make_response(body, "text/plain; charset=utf-8"))
    }

    pub(super) fn make_response<T>(body: T, content_type: &'static str) -> Response<T> {
        let mut response = Response::new(body);
        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::header::HeaderValue::from_static(content_type),
        );
        response
    }
}
