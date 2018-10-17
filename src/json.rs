//! Components for parsing JSON values and creating JSON responses.

use bytes::Bytes;
use http::header::{HeaderMap, HeaderValue};
use http::{header, Request, Response, StatusCode};
use mime;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use std::borrow::Cow;
use std::ops::Deref;

use error::ErrorHandler;
use error::{Error, Failure, HttpError, Never};
use input::body::{FromData, RequestBody};
use input::header::content_type;
use input::Input;
use modifier::{AfterHandle, Modifier};
use output::{Output, Responder, ResponseBody};

/// A trait representing additional information for constructing HTTP responses from `Json<T>`.
pub trait HttpResponse {
    /// Returns an HTTP status code associated with the value of this type.
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }

    /// Appends some entries into the header map of an HTTP response.
    #[allow(unused_variables)]
    fn append_headers(&self, headers: &mut HeaderMap) {}
}

macro_rules! impl_http_response {
    ($($t:ty,)*) => {$(
        impl HttpResponse for $t {}
    )*};
}

impl_http_response! {
    bool,
    char,
    str,
    String,
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    f32, f64,
}

impl<'a> HttpResponse for Cow<'a, str> {}

impl<T: Serialize> HttpResponse for Vec<T> {}

impl<T: Serialize> HttpResponse for [T] {}

impl HttpResponse for () {
    fn status_code(&self) -> StatusCode {
        StatusCode::NO_CONTENT
    }
}

impl<T> HttpResponse for Option<T>
where
    T: HttpResponse,
{
    fn status_code(&self) -> StatusCode {
        match self {
            Some(ref v) => v.status_code(),
            None => StatusCode::NOT_FOUND,
        }
    }

    fn append_headers(&self, h: &mut HeaderMap) {
        if let Some(ref v) = self {
            v.append_headers(h);
        }
    }
}

impl<T, E> HttpResponse for Result<T, E>
where
    T: HttpResponse,
    E: HttpResponse,
{
    fn status_code(&self) -> StatusCode {
        match self {
            Ok(ref v) => v.status_code(),
            Err(ref e) => e.status_code(),
        }
    }

    fn append_headers(&self, h: &mut HeaderMap) {
        match self {
            Ok(ref v) => v.append_headers(h),
            Err(ref e) => e.append_headers(h),
        }
    }
}

/// A wraper struct representing a statically typed JSON value.
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    #[allow(missing_docs)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Json<T> {
    fn from(val: T) -> Self {
        Json(val)
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: DeserializeOwned> FromData for Json<T> {
    type Error = Failure;

    fn from_data(data: Bytes, input: &mut Input) -> Result<Json<T>, Self::Error> {
        if let Some(mime) = content_type(input)? {
            if *mime != mime::APPLICATION_JSON {
                return Err(Failure::bad_request(format_err!(
                    "The value of Content-type is not equal to application/json"
                )));
            }
        }

        serde_json::from_slice(&*data)
            .map_err(Failure::bad_request)
            .map(Json)
    }
}

impl<T: Serialize + HttpResponse> Responder for Json<T> {
    type Body = Vec<u8>;
    type Error = Failure;

    fn respond_to(self, _: &mut Input) -> Result<Response<Self::Body>, Self::Error> {
        let body = serde_json::to_vec(&self.0).map_err(Failure::internal_server_error)?;
        let mut response = json_response(body);
        *response.status_mut() = self.0.status_code();
        self.0.append_headers(response.headers_mut());
        Ok(response)
    }
}

/// A general JSON value.
#[derive(Debug)]
pub struct JsonValue(serde_json::Value);

impl From<serde_json::Value> for JsonValue {
    fn from(val: serde_json::Value) -> JsonValue {
        JsonValue(val)
    }
}

impl Responder for JsonValue {
    type Body = String;
    type Error = Never;

    fn respond_to(self, _: &mut Input) -> Result<Response<Self::Body>, Self::Error> {
        Ok(json_response(self.0.to_string()))
    }
}

/// An error type representing a JSON error response.
#[derive(Debug, Fail)]
#[fail(display = "{}", cause)]
pub struct JsonError {
    cause: Box<dyn HttpError>,
}

impl JsonError {
    /// Creates a "JsonError" from the provided error value.
    pub fn new(cause: impl Into<Box<dyn HttpError>>) -> JsonError {
        JsonError {
            cause: cause.into(),
        }
    }

    /// Returns a reference to the internal error value.
    pub fn cause(&self) -> &dyn HttpError {
        &*self.cause
    }
}

impl<E> From<E> for JsonError
where
    E: Into<Box<dyn HttpError>>,
{
    fn from(cause: E) -> Self {
        JsonError::new(cause)
    }
}

impl HttpError for JsonError {
    fn status(&self) -> StatusCode {
        self.cause.status()
    }

    fn headers(&self, headers: &mut HeaderMap) {
        self.cause.headers(headers);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }

    fn body(&mut self, _: &Request<RequestBody>) -> ResponseBody {
        json!({
            "code": self.status().as_u16(),
            "description": self.to_string(),
        }).to_string()
        .into()
    }
}

/// An error handler for creating JSON error responses.
#[derive(Debug, Default)]
pub struct JsonErrorHandler {
    _priv: (),
}

impl JsonErrorHandler {
    #[allow(missing_docs)]
    pub fn new() -> JsonErrorHandler {
        Default::default()
    }
}

impl ErrorHandler for JsonErrorHandler {
    fn handle_error(&self, err: Error) -> Error {
        err.map(JsonError::new)
    }
}

impl Modifier for JsonErrorHandler {
    fn after_handle(&self, _: &mut Input, result: Result<Output, Error>) -> AfterHandle {
        AfterHandle::ready(result.map_err(|err| err.map(JsonError::new)))
    }
}

// ====

fn json_response<T>(body: T) -> Response<T> {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}
