//! Components for constructing HTTP responses.

use bytes::{Buf, Bytes, IntoBuf};
use either::Either;
use futures::{Poll, Stream};
use http::header::{HeaderMap, HeaderValue};
use http::{header, Response, StatusCode};
use serde::Serialize;

use crate::error::{Error, Never};
use crate::input::Input;
use crate::server::server::CritError;
use crate::server::service::http::{Body, Payload};

pub use crate::macros::Responder;

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

/// A function which creates a JSON responder.
pub fn json<T>(data: T) -> Json<T>
where
    T: Serialize,
{
    Json {
        data,
        pretty: false,
    }
}

/// A function which creates a JSON responder with pretty output.
pub fn json_pretty<T>(data: T) -> Json<T>
where
    T: Serialize,
{
    json(data).pretty()
}

/// A wraper struct representing a statically typed JSON value.
#[derive(Debug)]
pub struct Json<T> {
    data: T,
    pretty: bool,
}

impl<T> Json<T>
where
    T: Serialize,
{
    /// Enables pretty output.
    pub fn pretty(self) -> Self {
        Self {
            pretty: true,
            ..self
        }
    }
}

impl<T> From<T> for Json<T>
where
    T: Serialize,
{
    fn from(data: T) -> Self {
        Self {
            data,
            pretty: false,
        }
    }
}

impl<T> Responder for Json<T>
where
    T: Serialize,
{
    type Body = Vec<u8>;
    type Error = Error;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let body = if self.pretty {
            serde_json::to_vec_pretty(&self.data).map_err(crate::error::internal_server_error)?
        } else {
            serde_json::to_vec(&self.data).map_err(crate::error::internal_server_error)?
        };
        Ok(json_response(body))
    }
}

impl Responder for serde_json::Value {
    type Body = String;
    type Error = Never;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(json_response(self.to_string()))
    }
}

fn json_response<T>(body: T) -> Response<T> {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

#[allow(missing_docs)]
#[inline]
pub fn html<T>(body: T) -> Html<T>
where
    T: Into<ResponseBody>,
{
    Html(body)
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Html<T>(T);

impl<T> Responder for Html<T>
where
    T: Into<ResponseBody>,
{
    type Body = T;
    type Error = Never;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(Response::builder()
            .header("content-type", "text/html; charset=utf-8")
            .body(self.0)
            .expect("should be a valid response"))
    }
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
