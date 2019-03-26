//! Components for constructing HTTP responses.

pub mod body;
pub mod redirect;

pub use {
    self::body::ResponseBody, //
    tsukuyomi_macros::IntoResponse,
};

use {
    crate::error::HttpError, //
    failure::{AsFail, Fail},
    http::{Request, StatusCode},
    serde::Serialize,
    std::fmt,
};

// the private API for custom derive.
#[doc(hidden)]
pub mod internal {
    pub use {
        crate::output::{preset::Preset, Error, IntoResponse, Response, Result},
        http::Request,
    };
}

/// Type alias of `http::Response<T>` that fixed the body type to `ResponseBody.
pub type Response = http::Response<ResponseBody>;

/// The error type which occurs during creating HTTP responses.
#[derive(Debug)]
pub struct Error(failure::Error);

impl<E> From<E> for Error
where
    E: Into<failure::Error>,
{
    fn from(err: E) -> Self {
        Error(err.into())
    }
}

impl std::ops::Deref for Error {
    type Target = failure::Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "output error: {}", self.0)
    }
}

impl AsFail for Error {
    fn as_fail(&self) -> &dyn Fail {
        self.0.as_fail()
    }
}

impl HttpError for Error {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// The type alias of return value from `IntoResponse::into_response`.
pub type Result<T = Response> = std::result::Result<T, Error>;

/// A trait representing the conversion into an HTTP response.
///
/// # Derivation
///
/// The custom derive `IntoResponse` is provided for reduce boilerplates around
/// trait implementations.
///
/// The macro has a parameter `#[response(preset = "..")]`, which specifies
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
/// ## Notes
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
/// [`Preset`]: ./preset/trait.Preset.html
pub trait IntoResponse {
    /// Converts itself into an HTTP response.
    ///
    /// The generated response will change based of the request information.
    fn into_response(self, request: &Request<()>) -> Result;
}

impl IntoResponse for () {
    fn into_response(self, _: &Request<()>) -> Result {
        let mut response = Response::new(ResponseBody::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        Ok(response)
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self, _: &Request<()>) -> Result {
        let mut response = Response::new(ResponseBody::empty());
        *response.status_mut() = self;
        Ok(response)
    }
}

impl<T> IntoResponse for http::Response<T>
where
    T: Into<ResponseBody>,
{
    #[inline]
    fn into_response(self, _: &Request<()>) -> Result {
        Ok(self.map(Into::into))
    }
}

impl IntoResponse for &'static str {
    #[inline]
    fn into_response(self, _: &Request<()>) -> Result {
        let len = self.len() as u64;
        Ok(self::make_response(
            self,
            "text/plain; charset=utf-8",
            Some(len),
        ))
    }
}

impl IntoResponse for String {
    #[inline]
    fn into_response(self, _: &Request<()>) -> Result {
        let len = self.len() as u64;
        Ok(self::make_response(
            self,
            "text/plain; charset=utf-8",
            Some(len),
        ))
    }
}

impl IntoResponse for serde_json::Value {
    fn into_response(self, _: &Request<()>) -> Result {
        let body = self.to_string();
        let len = body.len() as u64;
        Ok(self::make_response(body, "application/json", Some(len)))
    }
}

/// Creates a JSON responder from the specified data.
#[inline]
pub fn json<T>(data: T) -> impl IntoResponse
where
    T: Serialize,
{
    self::preset::render::<_, self::preset::Json>(data)
}

/// Creates a JSON response with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> impl IntoResponse
where
    T: Serialize,
{
    self::preset::render::<_, self::preset::JsonPretty>(data)
}

/// Creates an HTML response using the specified data.
#[inline]
pub fn html<T>(data: T) -> impl IntoResponse
where
    T: Into<ResponseBody>,
{
    self::preset::render::<_, self::preset::Html>(data)
}

/// Create an instance of `Response<T>` with the provided body and content type.
fn make_response<T>(body: T, content_type: &'static str, len: Option<u64>) -> Response
where
    T: Into<ResponseBody>,
{
    let mut response = Response::new(body.into());
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
        super::{IntoResponse, ResponseBody, Result},
        http::Request,
        serde::Serialize,
        std::marker::PhantomData,
    };

    pub fn render<T, P>(t: T) -> impl IntoResponse
    where
        P: Preset<T>,
    {
        struct Render<T, P>(T, PhantomData<P>);

        impl<T, P> IntoResponse for Render<T, P>
        where
            P: Preset<T>,
        {
            fn into_response(self, request: &Request<()>) -> Result {
                P::into_response(self.0, request)
            }
        }

        Render(t, PhantomData::<P>)
    }

    /// A trait representing the *preset* for deriving the implementation of `IntoResponse`.
    pub trait Preset<T> {
        fn into_response(t: T, request: &Request<()>) -> Result;
    }

    #[allow(missing_debug_implementations)]
    pub struct Json(());

    impl<T> Preset<T> for Json
    where
        T: Serialize,
    {
        fn into_response(data: T, _: &Request<()>) -> Result {
            let body = serde_json::to_vec(&data)?;
            let len = body.len() as u64;
            Ok(super::make_response(body, "application/json", Some(len)))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct JsonPretty(());

    impl<T> Preset<T> for JsonPretty
    where
        T: Serialize,
    {
        fn into_response(data: T, _: &Request<()>) -> Result {
            let body = serde_json::to_vec_pretty(&data)?;
            let len = body.len() as u64;
            Ok(super::make_response(body, "application/json", Some(len)))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Html(());

    impl<T> Preset<T> for Html
    where
        T: Into<ResponseBody>,
    {
        fn into_response(body: T, _: &Request<()>) -> Result {
            Ok(super::make_response(body, "text/html", None))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Plain(());

    impl<T> Preset<T> for Plain
    where
        T: Into<ResponseBody>,
    {
        fn into_response(body: T, _: &Request<()>) -> Result {
            Ok(super::make_response(
                body,
                "text/plain; charset=utf-8",
                None,
            ))
        }
    }
}
