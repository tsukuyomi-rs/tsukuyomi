//! Components for constructing HTTP responses.

pub mod body;
pub mod redirect;

pub use {self::body::ResponseBody, tsukuyomi_macros::IntoResponse};

use {
    http::{Response, StatusCode},
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
    fn into_response(self) -> Response<ResponseBody>;
}

impl IntoResponse for () {
    fn into_response(self) -> Response<ResponseBody> {
        let mut response = Response::new(ResponseBody::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        response
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self) -> Response<ResponseBody> {
        let mut response = Response::new(ResponseBody::empty());
        *response.status_mut() = self;
        response
    }
}

impl<T> IntoResponse for Response<T>
where
    T: Into<ResponseBody>,
{
    #[inline]
    fn into_response(self) -> Response<ResponseBody> {
        self.map(Into::into)
    }
}

impl IntoResponse for &'static str {
    #[inline]
    fn into_response(self) -> Response<ResponseBody> {
        let len = self.len() as u64;
        self::make_response(self.into(), "text/plain; charset=utf-8", Some(len))
    }
}

impl IntoResponse for String {
    #[inline]
    fn into_response(self) -> Response<ResponseBody> {
        let len = self.len() as u64;
        self::make_response(self.into(), "text/plain; charset=utf-8", Some(len))
    }
}

impl IntoResponse for serde_json::Value {
    fn into_response(self) -> Response<ResponseBody> {
        let body = self.to_string();
        let len = body.len() as u64;
        self::make_response(body.into(), "application/json", Some(len))
    }
}

/// Creates a JSON responder from the specified data.
#[inline]
pub fn json<T>(data: T) -> Response<ResponseBody>
where
    T: Serialize,
{
    use self::preset::Preset;
    self::preset::Json::into_response(data)
}

/// Creates a JSON response with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> Response<ResponseBody>
where
    T: Serialize,
{
    use self::preset::Preset;
    self::preset::JsonPretty::into_response(data)
}

/// Creates an HTML response using the specified data.
#[inline]
pub fn html<T>(body: T) -> Response<T>
where
    T: Into<ResponseBody>,
{
    self::make_response(body, "text/html", None)
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
        http::{Response, StatusCode},
        serde::Serialize,
    };

    /// A trait representing the *preset* for deriving the implementation of `IntoResponse`.
    pub trait Preset<T> {
        fn into_response(t: T) -> Response<ResponseBody>;
    }

    #[allow(missing_debug_implementations)]
    pub struct Json(());

    impl<T> Preset<T> for Json
    where
        T: Serialize,
    {
        fn into_response(data: T) -> Response<ResponseBody> {
            match serde_json::to_vec(&data) {
                Ok(body) => {
                    let len = body.len() as u64;
                    super::make_response(body.into(), "application/json", Some(len))
                }
                Err(_e) => {
                    let mut res = Response::new(ResponseBody::empty());
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    res
                }
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct JsonPretty(());

    impl<T> Preset<T> for JsonPretty
    where
        T: Serialize,
    {
        fn into_response(data: T) -> Response<ResponseBody> {
            match serde_json::to_vec_pretty(&data) {
                Ok(body) => {
                    let len = body.len() as u64;
                    super::make_response(body.into(), "application/json", Some(len))
                }
                Err(_e) => {
                    let mut res = Response::new(ResponseBody::empty());
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    res
                }
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Html(());

    impl<T> Preset<T> for Html
    where
        T: Into<ResponseBody>,
    {
        fn into_response(body: T) -> Response<ResponseBody> {
            super::make_response(body.into(), "text/html", None)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Plain(());

    impl<T> Preset<T> for Plain
    where
        T: Into<ResponseBody>,
    {
        fn into_response(body: T) -> Response<ResponseBody> {
            super::make_response(body.into(), "text/plain; charset=utf-8", None)
        }
    }
}
