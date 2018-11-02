//! Components for supporting Askama template.

extern crate askama;

use self::askama::Template;
use http::header::{HeaderValue, CONTENT_TYPE};
use http::Response;
use mime_guess::get_mime_type_str;
use tsukuyomi::error::{Error, Failure};

pub use crate::macros::TemplateResponder as Responder;

/// A helper function to generate an HTTP response from Askama template.
pub fn respond(t: &dyn Template, ext: &str) -> Result<Response<String>, Error> {
    let content_type = get_mime_type_str(ext).unwrap_or("text/html; charset=utf-8");
    let mut response = t
        .render()
        .map(Response::new)
        .map_err(Failure::internal_server_error)?;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}

// not a public API.
#[doc(hidden)]
pub mod private {
    pub use super::askama::Template;
    pub use http::Response;
    pub use tsukuyomi::error::Error;
    pub use tsukuyomi::input::Input;
    pub use tsukuyomi::output::Responder;
}
