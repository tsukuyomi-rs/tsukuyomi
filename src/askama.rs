#![cfg(feature = "with-askama")]

//! Components for supporting Askama template.
//!
//! # Example
//!
//! ```ignore
//! extern crate askama;
//!
//! #[derive(
//!     Debug,
//!     askama::Template,
//!     tsukuyomi::askama::Responder,
//! )]
//! #[template(path = "index.html")]
//! struct IndexPage {
//!     name: &'static str,
//! }
//!
//! let app = App::builder()
//!     .route(
//!         route::index()
//!             .reply(|| {
//!                 IndexPage {
//!                     name: "Alice",
//!                 }
//!             )
//!     )
//!     .finish()?;
//! ```

extern crate askama;
extern crate mime_guess;

use self::askama::Template;
use self::mime_guess::get_mime_type_str;
use http::header::{HeaderValue, CONTENT_TYPE};
use http::Response;

use crate::error::{Error, Failure};

pub use crate::internal::TemplateResponder as Responder;
#[doc(no_inline)]
pub use crate::output::Responder;

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
    pub use crate::error::Error;
    pub use crate::input::Input;
    pub use crate::output::Responder;
    pub use http::Response;
}
