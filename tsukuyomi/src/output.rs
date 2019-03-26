//! Components for constructing HTTP responses.

pub mod body;
pub mod redirect;

pub use {
    self::body::ResponseBody, //
    tsukuyomi_macros::IntoResponse,
};

use {
    http::{Request, StatusCode},
    serde::Serialize,
};

// the private API for custom derive.
#[doc(hidden)]
pub mod internal {
    pub use {
        crate::{
            error::Result,
            output::{preset::Preset, IntoResponse, Response},
        },
        http::Request,
    };
}

/// Type alias of `http::Response<T>` that fixed the body type to `ResponseBody.
pub type Response = http::Response<ResponseBody>;

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
/// [`Preset`]: ./preset/trait.Preset.html
pub trait IntoResponse {
    /// Converts itself into an HTTP response.
    ///
    /// The generated response will change based of the request information.
    fn into_response(self, request: &Request<()>) -> crate::Result<Response>;
}

impl IntoResponse for () {
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
        let mut response = Response::new(ResponseBody::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        Ok(response)
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
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
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
        Ok(self.map(Into::into))
    }
}

impl IntoResponse for &'static str {
    #[inline]
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
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
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
        let len = self.len() as u64;
        Ok(self::make_response(
            self,
            "text/plain; charset=utf-8",
            Some(len),
        ))
    }
}

impl IntoResponse for serde_json::Value {
    fn into_response(self, _: &Request<()>) -> crate::Result<Response> {
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
        super::{IntoResponse, Response, ResponseBody},
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
            fn into_response(self, request: &Request<()>) -> crate::Result<Response> {
                P::into_response(self.0, request)
            }
        }

        Render(t, PhantomData::<P>)
    }

    /// A trait representing the *preset* for deriving the implementation of `IntoResponse`.
    pub trait Preset<T> {
        fn into_response(t: T, request: &Request<()>) -> crate::Result<Response>;
    }

    #[allow(missing_debug_implementations)]
    pub struct Json(());

    impl<T> Preset<T> for Json
    where
        T: Serialize,
    {
        fn into_response(data: T, _: &Request<()>) -> crate::Result<Response> {
            let body = serde_json::to_vec(&data).map_err(crate::error::internal_server_error)?;
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
        fn into_response(data: T, _: &Request<()>) -> crate::Result<Response> {
            let body =
                serde_json::to_vec_pretty(&data).map_err(crate::error::internal_server_error)?;
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
        fn into_response(body: T, _: &Request<()>) -> crate::Result<Response> {
            Ok(super::make_response(body, "text/html", None))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct Plain(());

    impl<T> Preset<T> for Plain
    where
        T: Into<ResponseBody>,
    {
        fn into_response(body: T, _: &Request<()>) -> crate::Result<Response> {
            Ok(super::make_response(
                body,
                "text/plain; charset=utf-8",
                None,
            ))
        }
    }
}
