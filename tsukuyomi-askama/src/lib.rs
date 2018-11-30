//! Askama integration for Tsukuyomi.
//!
//! ```
//! extern crate askama;
//! extern crate tsukuyomi;
//! extern crate tsukuyomi_askama;
//!
//! use askama::Template;
//! use tsukuyomi::output::Responder;
//!
//! #[derive(Template, Responder)]
//! #[template(source = "Hello, {{name}}!", ext = "html")]
//! #[responder(respond_to = "tsukuyomi_askama::respond_to")]
//! struct Index {
//!     name: String,
//! }
//!
//! # fn main() -> tsukuyomi::app::Result<()> {
//! tsukuyomi::App::builder()
//!     .with(
//!         tsukuyomi::app::scope::route!("/:name")
//!             .reply(|name| Index { name })
//!     )
//!     .build()
//! #   .map(drop)
//! # }
//! ```

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
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate askama;
extern crate http;
extern crate mime_guess;
extern crate tsukuyomi;

use {
    askama::Template,
    http::{
        header::{HeaderValue, CONTENT_TYPE},
        Response,
    },
    mime_guess::get_mime_type_str,
    tsukuyomi::{
        error::{internal_server_error, Result},
        input::Input,
    },
};

/// A helper function to generate an HTTP response from Askama template.
#[inline]
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
pub fn respond_to<T>(t: T, _: &mut Input<'_>) -> Result<Response<String>>
where
    T: Template,
{
    let content_type = t
        .extension()
        .and_then(get_mime_type_str)
        .unwrap_or("text/html; charset=utf-8");
    let mut response = t
        .render()
        .map(Response::new)
        .map_err(internal_server_error)?;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}
