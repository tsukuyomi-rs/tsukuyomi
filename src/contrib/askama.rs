//! Components for supporting Askama template.
//!
//! # Example
//!
//! ```ignore
//! use tsukuyomi::input::Input;
//! use tsukuyomi::output::Responder;
//! use tsukuyomi::contrib::askama::{Template, TemplateExt};
//!
//! #[derive(Debug, Template)]
//! struct IndexPage {
//!     name: String,
//! }
//!
//! fn index(_: &mut Input) -> impl Responder {
//!     (IndexPage {
//!         name: "Alice".into(),
//!     }).into_responder()
//! }
//! ```

use http::header::{HeaderValue, CONTENT_TYPE};
use http::Response;
use mime_guess::get_mime_type_str;

use crate::error::{Error, Failure};
use crate::input::Input;
use crate::output::Responder;

#[doc(no_inline)]
pub use askama::Template;

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

/// A wrapper struct for adding implementation of `Responder` to `T: Template`.
#[derive(Debug)]
pub struct Renderable<T: Template>(T);

impl<T> From<T> for Renderable<T>
where
    T: Template,
{
    fn from(ctx: T) -> Self {
        Renderable(ctx)
    }
}

impl<T> Responder for Renderable<T>
where
    T: Template,
{
    type Body = String;
    type Error = Error;

    fn respond_to(self, _: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::respond(&self.0, self.0.extension().unwrap_or("html"))
    }
}

#[allow(missing_docs)]
pub trait TemplateExt: Template + Sized {
    /// Convert itself into a `Renderable`.
    fn into_responder(self) -> Renderable<Self> {
        Renderable(self)
    }
}

impl<T> TemplateExt for T where T: Template {}
