use bytes::Bytes;
use http::header::HeaderValue;
use http::{header, Request, Response, StatusCode};
use mime::{self, Mime};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use std::ops::Deref;

use error::Error;
use input::body::FromData;
use output::{Output, Responder, ResponseBody};

#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
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

impl<T: DeserializeOwned + 'static> FromData for Json<T> {
    fn from_data<U>(data: Bytes, request: &Request<U>) -> Result<Json<T>, Error> {
        if let Some(h) = request.headers().get(header::CONTENT_TYPE) {
            let mime: Mime = h.to_str().map_err(bad_request)?.parse().map_err(bad_request)?;
            if mime != mime::APPLICATION_JSON {
                return Err(bad_request(format_err!(
                    "The value of Content-type is not equal to application/json"
                )));
            }
        }

        serde_json::from_slice(&*data)
            .map_err(|e| Error::new(e, StatusCode::BAD_REQUEST))
            .map(Json)
    }
}

impl<T: Serialize> Responder for Json<T> {
    fn respond_to<U>(self, _: &Request<U>) -> Result<Output, Error> {
        let body = serde_json::to_vec(&self.0)?;
        Ok(json_response(body))
    }
}

#[derive(Debug)]
pub struct JsonValue(serde_json::Value);

impl From<serde_json::Value> for JsonValue {
    fn from(val: serde_json::Value) -> JsonValue {
        JsonValue(val)
    }
}

impl Responder for JsonValue {
    fn respond_to<T>(self, _: &Request<T>) -> Result<Output, Error> {
        Ok(json_response(self.0.to_string()))
    }
}

fn json_response<T: Into<ResponseBody>>(body: T) -> Output {
    let mut response = Response::new(body.into());
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response.into()
}

fn bad_request<E>(err: E) -> Error
where
    E: Into<::failure::Error>,
{
    Error::new(err, StatusCode::BAD_REQUEST)
}
