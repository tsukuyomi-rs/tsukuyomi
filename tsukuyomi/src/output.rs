//! Components for constructing HTTP responses.

pub use tsukuyomi_macros::IntoResponse;

use {
    crate::{
        core::Never,
        error::Error,
        input::{body::RequestBody, Input},
    },
    bytes::{Buf, Bytes, IntoBuf},
    futures01::{
        future::{self, FutureResult},
        Future, IntoFuture, Poll, Stream,
    },
    http::{header::HeaderMap, Response, StatusCode},
    hyper::body::{Body, Payload},
    serde::Serialize,
};

// the private API for custom derive.
#[doc(hidden)]
pub mod internal {
    use crate::{
        error::Error,
        input::Input,
        output::{IntoResponse, ResponseBody},
    };
    pub use http::Response;

    #[inline]
    pub fn into_response<T>(t: T, input: &mut Input<'_>) -> Result<Response<ResponseBody>, Error>
    where
        T: IntoResponse,
    {
        IntoResponse::into_response(t, input)
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
        S::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
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

pub trait IntoResponse {
    type Body: Into<ResponseBody>;
    type Error: Into<Error>;

    fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error>;
}

impl IntoResponse for () {
    type Body = ();
    type Error = Never;

    fn into_response(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let mut response = Response::new(());
        *response.status_mut() = StatusCode::NO_CONTENT;
        Ok(response)
    }
}

impl<T> IntoResponse for Option<T>
where
    T: IntoResponse,
{
    type Body = ResponseBody;
    type Error = Error;

    fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let x = self.ok_or_else(|| crate::error::not_found("None"))?;
        x.into_response(input)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
    }
}

impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: Into<Error>,
{
    type Body = ResponseBody;
    type Error = Error;

    fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self.map_err(Into::into)?
            .into_response(input)
            .map(|res| res.map(Into::into))
            .map_err(Into::into)
    }
}

impl<T> IntoResponse for Response<T>
where
    T: Into<ResponseBody>,
{
    type Body = T;
    type Error = Never;

    #[inline]
    fn into_response(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self)
    }
}

impl IntoResponse for &'static str {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response::plain(self, input)
    }
}

impl IntoResponse for String {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response::plain(self, input)
    }
}

impl IntoResponse for serde_json::Value {
    type Body = String;
    type Error = Never;

    fn into_response(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self::into_response::make_response(
            self.to_string(),
            "application/json",
        ))
    }
}

/// A function to create a `IntoResponse` using the specified function.
pub fn into_response<T, E>(
    f: impl FnOnce(&mut Input<'_>) -> Result<Response<T>, E>,
) -> impl IntoResponse<
    Body = T, //
    Error = E,
>
where
    T: Into<ResponseBody>,
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    pub struct IntoResponseFn<F>(F);

    impl<F, T, E> IntoResponse for IntoResponseFn<F>
    where
        F: FnOnce(&mut Input<'_>) -> Result<Response<T>, E>,
        T: Into<ResponseBody>,
        E: Into<Error>,
    {
        type Body = T;
        type Error = E;

        #[inline]
        fn into_response(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
            (self.0)(input)
        }
    }

    IntoResponseFn(f)
}

/// Creates a JSON responder from the specified data.
#[inline]
pub fn json<T>(data: T) -> impl IntoResponse<Body = Vec<u8>, Error = Error>
where
    T: Serialize,
{
    self::into_response(move |input| self::into_response::json(data, input))
}

/// Creates a JSON responder with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> impl IntoResponse<Body = Vec<u8>, Error = Error>
where
    T: Serialize,
{
    self::into_response(move |input| self::into_response::json_pretty(data, input))
}

/// Creates an HTML responder with the specified response body.
#[inline]
pub fn html<T>(body: T) -> impl IntoResponse<Body = T, Error = Never>
where
    T: Into<ResponseBody>,
{
    self::into_response(move |input| self::into_response::html(body, input))
}

#[allow(missing_docs)]
pub mod into_response {
    use {
        super::ResponseBody,
        crate::{core::Never, error::Error, input::Input},
        http::Response,
        serde::Serialize,
    };

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

#[allow(missing_docs)]
pub mod redirect {
    use {
        super::*,
        http::{Response, StatusCode},
        std::borrow::Cow,
    };

    #[derive(Debug, Clone)]
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

    impl IntoResponse for Redirect {
        type Body = ();
        type Error = Never;

        #[inline]
        fn into_response(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
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

// ==== Responder ====

/// A trait representing the conversion to an HTTP response.
pub trait Responder {
    /// The type of message body in the generated HTTP response.
    type Body: Into<ResponseBody>;

    /// The error type which will be returned from `respond_to`.
    type Error: Into<Error>;

    /// The type of `Future` which will be returned from `respond_to`.
    type Future: Future<Item = Response<Self::Body>, Error = Self::Error> + Send + 'static;

    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input<'_>) -> Self::Future;
}

/// a branket impl of `Responder` for `IntoResponse`s.
impl<T> Responder for T
where
    T: IntoResponse,
    T::Body: Send + 'static,
    T::Error: Send + 'static,
{
    type Body = T::Body;
    type Error = T::Error;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    #[inline]
    fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
        future::result(self.into_response(input))
    }
}

mod impl_responder_for_either {
    use {
        super::{Responder, ResponseBody},
        crate::{error::Error, input::Input},
        either::Either,
        futures01::{Future, Poll},
        http::Response,
    };

    impl<L, R> Responder for Either<L, R>
    where
        L: Responder,
        R: Responder,
    {
        type Body = ResponseBody;
        type Error = Error;
        type Future = EitherFuture<L::Future, R::Future>;

        fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
            match self {
                Either::Left(l) => EitherFuture::Left(l.respond_to(input)),
                Either::Right(r) => EitherFuture::Right(r.respond_to(input)),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub enum EitherFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R, A, B> Future for EitherFuture<L, R>
    where
        L: Future<Item = Response<A>>,
        R: Future<Item = Response<B>>,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
        A: Into<ResponseBody>,
        B: Into<ResponseBody>,
    {
        type Item = Response<ResponseBody>;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self {
                EitherFuture::Left(l) => l
                    .poll()
                    .map(|x| x.map(|res| res.map(Into::into)))
                    .map_err(Into::into),
                EitherFuture::Right(r) => r
                    .poll()
                    .map(|x| x.map(|res| res.map(Into::into)))
                    .map_err(Into::into),
            }
        }
    }
}

/// A function to create a `Responder` using the specified function.
pub fn respond<R, T, E>(
    f: impl FnOnce(&mut Input<'_>) -> R,
) -> impl Responder<
    Body = T, //
    Error = E,
    Future = R::Future,
>
where
    R: IntoFuture<Item = Response<T>, Error = E>,
    R::Future: Send + 'static,
    T: Into<ResponseBody>,
    E: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    pub struct ResponderFn<F>(F);

    impl<F, R, T, E> Responder for ResponderFn<F>
    where
        F: FnOnce(&mut Input<'_>) -> R,
        R: IntoFuture<Item = Response<T>, Error = E>,
        R::Future: Send + 'static,
        T: Into<ResponseBody>,
        E: Into<Error>,
    {
        type Body = T;
        type Error = E;
        type Future = R::Future;

        #[inline]
        fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
            (self.0)(input).into_future()
        }
    }

    ResponderFn(f)
}
