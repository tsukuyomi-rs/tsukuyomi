//! Components for parsing JSON values and creating JSON responses.

use bytes::Bytes;
use http::header::{HeaderMap, HeaderValue};
use http::{header, Request, Response, StatusCode};
use mime;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use std::ops::Deref;

use error::handler::ErrorHandler;
use error::{CritError, Error, HttpError};
use input::body::FromData;
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
    fn from_data(data: Bytes, input: &mut Input) -> Result<Json<T>, Error> {
        if let Some(mime) = content_type(input)? {
            if *mime != mime::APPLICATION_JSON {
                return Err(Error::bad_request(format_err!(
                    "The value of Content-type is not equal to application/json"
                )));
            }
        }

        serde_json::from_slice(&*data).map_err(Error::bad_request).map(Json)
    }
}

impl<T: Serialize + HttpResponse> Responder for Json<T> {
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        let body = serde_json::to_vec(&self.0).map_err(Error::internal_server_error)?;
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
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(json_response(self.0.to_string()))
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

    fn make_error_response(&self, e: &dyn HttpError) -> Result<Response<ResponseBody>, CritError> {
        let body = json!({
            "code": e.status_code().as_u16(),
            "description": e.to_string(),
        }).to_string();

        Response::builder()
            .status(e.status_code())
            .header(header::CONNECTION, "close")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body.into())
            .map_err(Into::into)
    }
}

impl ErrorHandler for JsonErrorHandler {
    fn handle_error(&self, err: &dyn HttpError, _: &Request<()>) -> Result<Response<ResponseBody>, CritError> {
        self.make_error_response(err)
    }
}

impl Modifier for JsonErrorHandler {
    fn after_handle(&self, _: &mut Input, result: Result<Output, Error>) -> AfterHandle {
        AfterHandle::ready(match result {
            Ok(output) => Ok(output),
            Err(ref e) if !e.is_critical() => self.make_error_response(e.as_http_error().unwrap())
                .map(Into::into)
                .map_err(Error::critical),
            Err(e) => Err(e),
        })
    }
}

// ====

fn json_response<T: Into<ResponseBody>>(body: T) -> Response<ResponseBody> {
    let mut response = Response::new(body.into());
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}
