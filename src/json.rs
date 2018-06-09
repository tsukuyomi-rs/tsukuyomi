use http::header::HeaderValue;
use http::{header, Request, Response};
use serde::ser::Serialize;
use serde_json;

use error::Error;
use output::{Output, Responder, ResponseBody};

#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> From<T> for Json<T> {
    fn from(val: T) -> Self {
        Json(val)
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
