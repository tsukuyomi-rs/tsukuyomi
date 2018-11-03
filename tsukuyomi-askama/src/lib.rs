//! Components for supporting Askama template.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-askama/0.1.0")]
#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]

extern crate tsukuyomi_askama_macros as macros;

extern crate askama;
extern crate http;
extern crate mime_guess;
extern crate tsukuyomi;

use askama::Template;
use http::header::{HeaderValue, CONTENT_TYPE};
use http::Response;
use mime_guess::get_mime_type_str;
use tsukuyomi::error::Error;

pub use crate::macros::TemplateResponder as Responder;

/// A helper function to generate an HTTP response from Askama template.
pub fn respond(t: &dyn Template, ext: &str) -> Result<Response<String>, Error> {
    let content_type = get_mime_type_str(ext).unwrap_or("text/html; charset=utf-8");
    let mut response = t
        .render()
        .map(Response::new)
        .map_err(tsukuyomi::error::internal_server_error)?;
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
