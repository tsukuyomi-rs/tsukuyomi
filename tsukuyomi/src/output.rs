//! Components for constructing HTTP responses.

pub mod body;
pub mod redirect;

pub use {self::body::ResponseBody, tsukuyomi_macros::IntoResponse};

use {
    crate::{error::Error, util::Never},
    http::{Request, Response, StatusCode},
    serde::Serialize,
};

// the private API for custom derive.
#[doc(hidden)]
pub mod internal {
    pub use {
        crate::{
            error::Error,
            output::{preset::Preset, IntoResponse, ResponseBody},
        },
        http::{Request, Response},
    };
}

/// A trait representing the conversion into an HTTP response.
///
/// # Examples
///
/// This macro has a parameter `#[response(preset = "..")]`, which specifies
/// the path to a type that implements a trait [`Preset`]:
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// use serde::Serialize;
///
/// #[derive(Debug, Serialize, IntoResponse)]
/// #[response(preset = "tsukuyomi::output::preset::Json")]
/// struct Post {
///     title: String,
///     text: String,
/// }
/// # fn main() {}
/// ```
///
/// You can specify the additional trait bounds to type parameters
/// by using the parameter `#[response(bound = "..")]`:
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// # use serde::Serialize;
/// #[derive(Debug, IntoResponse)]
/// #[response(
///     preset = "tsukuyomi::output::preset::Json",
///     bound = "T: Serialize",
///     bound = "U: Serialize",
/// )]
/// struct CustomValue<T, U> {
///     t: T,
///     u: U,
/// }
/// # fn main() {}
/// ```
///
/// # Notes
/// 1. When `preset = ".."` is omitted for struct, a field in the specified
///    struct is chosen and the the implementation of `IntoResponse` for its
///    type is used. For example, the impls derived to the following types
///    outputs eventually the same result as the implementation of
///    `IntoResponse` for `String`:
///    ```
///    # use tsukuyomi::IntoResponse;
///    #[derive(IntoResponse)]
///    struct Foo(String);
///   
///    #[derive(IntoResponse)]
///    struct Bar {
///        inner: String,
///    }
///    ```
/// 1. When `preset = ".."` is omitted for enum, the same rule as struct is
///    applied to each variant:
///    ```
///    # use tsukuyomi::IntoResponse;
///    # use tsukuyomi::vendor::http::Response;
///    #[derive(IntoResponse)]
///    enum MyResponse {
///        Text(String),
///        Raw { response: Response<String> },
///    }
///    ```
/// 1. Without specifying the preset, the number of fields in the struct
///    or the number of fields of each variant inside of the enum must be
///    at most one.  This is because the field that implements `IntoResponse`
///    cannot be determined if there are two or more fields in a struct or
///    a variant:
///    ```compile_fail
///    # use tsukuyomi::IntoResponse;
///    #[derive(IntoResponse)]
///    enum ApiResponse {
///        Text(String),
///        Post { title: String, text: String },
///    }
///    ```
///    If you want to apply the derivation to complex enums,
///    consider cutting each variant into one struct and specifying
///    the preset explicitly as follows:
///    ```
///    # use tsukuyomi::IntoResponse;
///    # use serde::Serialize;
///    #[derive(IntoResponse)]
///    enum ApiResponse {
///        Text(String),
///        Post(Post),
///    }
///
///    #[derive(Debug, Serialize, IntoResponse)]
///    #[response(preset = "tsukuyomi::output::preset::Json")]
///    struct Post {
///         title: String,
///         text: String,
///    }
///    ```
///
/// [`Preset`]: https://tsukuyomi-rs.github.io/tsukuyomi/tsukuyomi/output/preset/trait.Preset.html
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
    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        let len = self.len() as u64;
        Ok(self::make_response(
            self,
            "text/plain; charset=utf-8",
            Some(len),
        ))
    }
}

impl IntoResponse for String {
    type Body = Self;
    type Error = Never;

    #[inline]
    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        let len = self.len() as u64;
        Ok(self::make_response(
            self,
            "text/plain; charset=utf-8",
            Some(len),
        ))
    }
}

impl IntoResponse for serde_json::Value {
    type Body = String;
    type Error = Never;

    fn into_response(self, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        let body = self.to_string();
        let len = body.len() as u64;
        Ok(self::make_response(body, "application/json", Some(len)))
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
    self::into_response(move |_| {
        serde_json::to_vec(&data)
            .map(|body| {
                let len = body.len() as u64;
                self::make_response(body, "application/json", Some(len))
            })
            .map_err(crate::error::internal_server_error)
    })
}

/// Creates a JSON responder with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> impl IntoResponse<Body = Vec<u8>, Error = Error>
where
    T: Serialize,
{
    self::into_response(move |_| {
        serde_json::to_vec_pretty(&data)
            .map(|body| {
                let len = body.len() as u64;
                self::make_response(body, "application/json", Some(len))
            })
            .map_err(crate::error::internal_server_error)
    })
}

/// Creates an HTML responder with the specified response body.
#[inline]
pub fn html<T>(body: T) -> impl IntoResponse<Body = T, Error = Never>
where
    T: Into<ResponseBody>,
{
    self::into_response(move |_| Ok(self::make_response(body, "text/html", None)))
}

/// Create an instance of `Response<T>` with the provided body and content type.
fn make_response<T>(body: T, content_type: &'static str, len: Option<u64>) -> Response<T> {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::header::HeaderValue::from_static(content_type),
    );
    if let Some(len) = len {
        response.headers_mut().insert(
            http::header::CONTENT_LENGTH,
            len.to_string()
                .parse()
                .expect("should be a valid header value"),
        );
    }
    response
}

pub mod preset {
    use {
        super::ResponseBody,
        crate::{error::Error, util::Never},
        http::{Request, Response},
        serde::Serialize,
    };

    /// A trait representing the *preset* for deriving the implementation of `IntoResponse`.
    pub trait Preset<T> {
        type Body: Into<ResponseBody>;
        type Error: Into<Error>;

        fn into_response(t: T, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error>;
    }

    #[allow(missing_debug_implementations)]
    pub struct Json(());

    impl<T> Preset<T> for Json
    where
        T: Serialize,
    {
        type Body = Vec<u8>;
        type Error = Error;

        fn into_response(data: T, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            serde_json::to_vec(&data)
                .map(|body| {
                    let len = body.len() as u64;
                    super::make_response(body, "application/json", Some(len))
                })
                .map_err(crate::error::internal_server_error)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct JsonPretty(());

    impl<T> Preset<T> for JsonPretty
    where
        T: Serialize,
    {
        type Body = Vec<u8>;
        type Error = Error;

        fn into_response(data: T, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            serde_json::to_vec_pretty(&data)
                .map(|body| {
                    let len = body.len() as u64;
                    super::make_response(body, "application/json", Some(len))
                })
                .map_err(crate::error::internal_server_error)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Html(());

    impl<T> Preset<T> for Html
    where
        T: Into<ResponseBody>,
    {
        type Body = T;
        type Error = Never;

        fn into_response(body: T, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            Ok(super::make_response(body, "text/html", None))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Plain(());

    impl<T> Preset<T> for Plain
    where
        T: Into<ResponseBody>,
    {
        type Body = T;
        type Error = Never;

        fn into_response(body: T, _: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
            Ok(super::make_response(
                body,
                "text/plain; charset=utf-8",
                None,
            ))
        }
    }
}
