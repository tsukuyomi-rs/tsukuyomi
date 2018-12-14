//! Components for constructing HTTP responses.

//pub use tsukuyomi_macros::Responder;

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

// not a public API.
// #[doc(hidden)]
// pub mod internal {
//     use crate::{
//         error::Error,
//         input::Input,
//         output::{Responder, ResponseBody},
//     };
//     pub use http::Response;

//     #[inline]
//     pub fn respond_to<T>(t: T, input: &mut Input<'_>) -> Result<Response<ResponseBody>, Error>
//     where
//         T: Responder,
//     {
//         Responder::respond_to(t, input)
//             .map(|resp| resp.map(Into::into))
//             .map_err(Into::into)
//     }
// }

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

#[cfg(feature = "tower-middleware")]
mod tower {
    use {super::*, tower_web::util::BufStream};

    impl BufStream for ResponseBody {
        type Item = hyper::Chunk;
        type Error = hyper::Error;

        #[inline]
        fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
            BufStream::poll(&mut self.0)
        }

        fn size_hint(&self) -> tower_web::util::buf_stream::SizeHint {
            self.0.size_hint()
        }
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

    /// The type of `Future` which will be returned from `respond_to`.
    type Future: Future<Item = Response<Self::Body>, Error = Self::Error> + Send + 'static;

    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input<'_>) -> Self::Future;
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

impl Responder for () {
    type Body = ();
    type Error = Never;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    fn respond_to(self, _: &mut Input<'_>) -> Self::Future {
        let mut response = Response::new(());
        *response.status_mut() = StatusCode::NO_CONTENT;
        future::ok(response)
    }
}

mod impl_responder_for_option {
    use {
        super::{Responder, ResponseBody},
        crate::{error::Error, input::Input},
        futures01::{Future, Poll},
        http::Response,
    };

    impl<T> Responder for Option<T>
    where
        T: Responder,
    {
        type Body = ResponseBody;
        type Error = Error;
        type Future = OptionFuture<T::Future>;

        fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
            OptionFuture(self.map(|x| x.respond_to(input)))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct OptionFuture<T>(Option<T>);

    impl<T, Bd> Future for OptionFuture<T>
    where
        T: Future<Item = Response<Bd>>,
        T::Error: Into<Error>,
        Bd: Into<ResponseBody>,
    {
        type Item = Response<ResponseBody>;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self.0 {
                Some(ref mut respond) => respond
                    .poll()
                    .map(|x| x.map(|res| res.map(Into::into)))
                    .map_err(Into::into),
                None => Err(crate::error::not_found("None")),
            }
        }
    }
}

mod impl_responder_for_result {
    use {
        super::{Responder, ResponseBody},
        crate::{error::Error, input::Input},
        futures01::{Future, Poll},
        http::Response,
    };

    impl<T, E> Responder for Result<T, E>
    where
        T: Responder,
        E: Into<Error> + Send + 'static,
    {
        type Body = ResponseBody;
        type Error = Error;
        type Future = ResultFuture<T::Future, E>;

        fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
            ResultFuture {
                inner: self.map(|x| x.respond_to(input)).map_err(Some),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct ResultFuture<T, E> {
        inner: Result<T, Option<E>>,
    }

    impl<T, Bd, E> Future for ResultFuture<T, E>
    where
        T: Future<Item = Response<Bd>>,
        T::Error: Into<Error>,
        Bd: Into<ResponseBody>,
        E: Into<Error> + Send + 'static,
    {
        type Item = Response<ResponseBody>;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self.inner {
                Ok(ref mut respond) => respond
                    .poll()
                    .map(|x| x.map(|res| res.map(Into::into)))
                    .map_err(Into::into),
                Err(ref mut err) => Err(err.take().expect("the future has already polled").into()),
            }
        }
    }
}

impl<T> Responder for Response<T>
where
    T: Into<ResponseBody> + Send + 'static,
{
    type Body = T;
    type Error = Never;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    #[inline]
    fn respond_to(self, _: &mut Input<'_>) -> Self::Future {
        future::ok(self)
    }
}

impl Responder for &'static str {
    type Body = Self;
    type Error = Never;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    #[inline]
    fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
        future::result(self::responder::plain(self, input))
    }
}

impl Responder for String {
    type Body = Self;
    type Error = Never;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    #[inline]
    fn respond_to(self, input: &mut Input<'_>) -> Self::Future {
        future::result(self::responder::plain(self, input))
    }
}

impl Responder for serde_json::Value {
    type Body = String;
    type Error = Never;
    type Future = FutureResult<Response<Self::Body>, Self::Error>;

    fn respond_to(self, _: &mut Input<'_>) -> Self::Future {
        future::ok(self::responder::make_response(
            self.to_string(),
            "application/json",
        ))
    }
}

/// Creates an instance of `Responder` from the specified function.
pub fn responder<F, R, T, E>(f: F) -> impl Responder
where
    F: FnOnce(&mut Input<'_>) -> R,
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
    T: Into<ResponseBody> + Send + 'static,
{
    self::responder(move |input| self::responder::html(body, input))
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

    impl Responder for Redirect {
        type Body = ();
        type Error = Never;
        type Future = futures01::future::FutureResult<Response<Self::Body>, Self::Error>;

        #[inline]
        fn respond_to(self, _: &mut Input<'_>) -> Self::Future {
            futures01::future::ok(
                Response::builder()
                    .status(self.status)
                    .header("location", &*self.location)
                    .body(())
                    .expect("should be a valid response"),
            )
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
