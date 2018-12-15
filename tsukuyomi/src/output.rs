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
    http::{header::HeaderMap, Request, Response, StatusCode},
    hyper::body::{Body, Payload},
    serde::Serialize,
};

// the private API for custom derive.
#[doc(hidden)]
pub mod internal {
    pub use {
        crate::{
            error::Error,
            output::{IntoResponse, ResponseBody},
        },
        http::{Request, Response},
    };

    #[inline]
    pub fn into_response<T>(t: T, request: &Request<()>) -> Result<Response<ResponseBody>, Error>
    where
        T: IntoResponse,
    {
        IntoResponse::into_response(t, request)
            .map(|response| response.map(Into::into))
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

/// A trait representing the conversion into an HTTP response.
pub trait IntoResponse {
    type Body: Into<ResponseBody>;
    type Error: Into<Error>;

    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error>;
}

impl IntoResponse for () {
    type Body = ();
    type Error = Never;

    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
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

    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        let x = self.ok_or_else(|| crate::error::not_found("None"))?;
        x.into_response(request)
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

    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        self.map_err(Into::into)?
            .into_response(request)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
    }
}

mod impl_into_response_for_either {
    use {super::*, either::Either};

    impl<L, R> IntoResponse for Either<L, R>
    where
        L: IntoResponse,
        R: IntoResponse,
    {
        type Body = ResponseBody;
        type Error = Error;

        fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            match self {
                Either::Left(l) => l
                    .into_response(request)
                    .map(|response| response.map(Into::into))
                    .map_err(Into::into),
                Either::Right(r) => r
                    .into_response(request)
                    .map(|response| response.map(Into::into))
                    .map_err(Into::into),
            }
        }
    }
}

impl<T> IntoResponse for Response<T>
where
    T: Into<ResponseBody>,
{
    type Body = T;
    type Error = Never;

    #[inline]
    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self)
    }
}

impl IntoResponse for &'static str {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response::plain(self, request)
    }
}

impl IntoResponse for String {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response::plain(self, request)
    }
}

impl IntoResponse for serde_json::Value {
    type Body = String;
    type Error = Never;

    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        Ok(self::into_response::make_response(
            self.to_string(),
            "application/json",
        ))
    }
}

/// A function to create a `IntoResponse` using the specified function.
pub fn into_response<T, E>(
    f: impl FnOnce(&Request<()>) -> Result<Response<T>, E>,
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
        F: FnOnce(&Request<()>) -> Result<Response<T>, E>,
        T: Into<ResponseBody>,
        E: Into<Error>,
    {
        type Body = T;
        type Error = E;

        #[inline]
        fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            (self.0)(request)
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
    self::into_response(move |request| self::into_response::json(data, request))
}

/// Creates a JSON responder with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> impl IntoResponse<Body = Vec<u8>, Error = Error>
where
    T: Serialize,
{
    self::into_response(move |request| self::into_response::json_pretty(data, request))
}

/// Creates an HTML responder with the specified response body.
#[inline]
pub fn html<T>(body: T) -> impl IntoResponse<Body = T, Error = Never>
where
    T: Into<ResponseBody>,
{
    self::into_response(move |request| self::into_response::html(body, request))
}

#[allow(missing_docs)]
pub mod into_response {
    use {
        super::ResponseBody,
        crate::{core::Never, error::Error},
        http::{Request, Response},
        serde::Serialize,
    };

    #[inline]
    pub fn json<T>(data: T, _: &Request<()>) -> Result<Response<Vec<u8>>, Error>
    where
        T: Serialize,
    {
        serde_json::to_vec(&data)
            .map(|body| self::make_response(body, "application/json"))
            .map_err(crate::error::internal_server_error)
    }

    #[inline]
    pub fn json_pretty<T>(data: T, _: &Request<()>) -> Result<Response<Vec<u8>>, Error>
    where
        T: Serialize,
    {
        serde_json::to_vec_pretty(&data)
            .map(|body| self::make_response(body, "application/json"))
            .map_err(crate::error::internal_server_error)
    }

    #[inline]
    pub fn html<T>(body: T, _: &Request<()>) -> Result<Response<T>, Never>
    where
        T: Into<ResponseBody>,
    {
        Ok(self::make_response(body, "text/html"))
    }

    #[inline]
    pub fn plain<T>(body: T, _: &Request<()>) -> Result<Response<T>, Never>
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
        fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
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

/// A trait representing a reply to the client.
pub trait Responder {
    /// The type of response
    type Response: IntoResponse;

    /// The error type which will be returned from `respond_to`.
    type Error: Into<Error>;

    /// The type of `Future` which will be returned from `respond_to`.
    type Future: Future<Item = Self::Response, Error = Self::Error> + Send + 'static;

    /// Converts itself into a `Future` that will be resolved as a `Response`.
    fn respond(self, input: &mut Input<'_>) -> Self::Future;
}

/// a branket impl of `Responder` for `IntoResponse`s.
impl<T> Responder for T
where
    T: IntoResponse + Send + 'static,
{
    type Response = T;
    type Error = Never;
    type Future = FutureResult<Self::Response, Self::Error>;

    #[inline]
    fn respond(self, _: &mut Input<'_>) -> Self::Future {
        future::ok(self)
    }
}

mod impl_responder_for_either {
    use {
        super::{IntoResponse, Responder},
        crate::{core::Either, error::Error, input::Input},
        futures01::{Future, Poll},
    };

    impl<L, R> Responder for Either<L, R>
    where
        L: Responder,
        R: Responder,
    {
        type Response = either::Either<L::Response, R::Response>;
        type Error = Error;
        type Future = EitherFuture<L::Future, R::Future>;

        fn respond(self, input: &mut Input<'_>) -> Self::Future {
            match self {
                Either::Left(l) => EitherFuture::Left(l.respond(input)),
                Either::Right(r) => EitherFuture::Right(r.respond(input)),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub enum EitherFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> Future for EitherFuture<L, R>
    where
        L: Future,
        R: Future,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
        L::Item: IntoResponse,
        R::Item: IntoResponse,
    {
        type Item = either::Either<L::Item, R::Item>;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self {
                EitherFuture::Left(l) => l
                    .poll()
                    .map(|x| x.map(either::Either::Left))
                    .map_err(Into::into),
                EitherFuture::Right(r) => r
                    .poll()
                    .map(|x| x.map(either::Either::Right))
                    .map_err(Into::into),
            }
        }
    }
}

/// A function to create a `Responder` using the specified function.
pub fn respond<R>(
    f: impl FnOnce(&mut Input<'_>) -> R,
) -> impl Responder<
    Response = R::Item, //
    Error = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Item: IntoResponse,
    R::Error: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    pub struct ResponderFn<F>(F);

    impl<F, R> Responder for ResponderFn<F>
    where
        F: FnOnce(&mut Input<'_>) -> R,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Item: IntoResponse,
        R::Error: Into<Error>,
    {
        type Response = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        #[inline]
        fn respond(self, input: &mut Input<'_>) -> Self::Future {
            (self.0)(input).into_future()
        }
    }

    ResponderFn(f)
}
