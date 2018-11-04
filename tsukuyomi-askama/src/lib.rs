//! Components for integrating the Askama template engine into Tsukuyomi.
//!
//! ```
//! extern crate askama;
//! extern crate tsukuyomi;
//! extern crate tsukuyomi_askama;
//!
//! use askama::Template;
//! use tsukuyomi::app::App;
//! use tsukuyomi::route;
//!
//! #[derive(
//!     askama::Template,
//!     tsukuyomi_askama::TemplateResponder,
//! )]
//! #[template(source = "Hello, {{name}}!", ext = "html")]
//! struct Index {
//!     name: String,
//! }
//!
//! # fn main() -> tsukuyomi::app::AppResult<()> {
//! let app = App::builder()
//!     .route(
//!         route::get!("/<name:String>")
//!             .reply(|name| Index { name })
//!     )
//!     .finish()?;
//! # drop(app);
//! # Ok(())
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

extern crate tsukuyomi_askama_macros as macros;

extern crate askama;
extern crate http;
extern crate mime_guess;
extern crate tsukuyomi;

pub use crate::macros::TemplateResponder;
pub use crate::private::TemplateResponder;

// not a public API.
#[doc(hidden)]
pub mod private {
    pub use askama::Template;
    pub use http::Response;
    pub use tsukuyomi::error::Error;
    pub use tsukuyomi::input::Input;
    pub use tsukuyomi::output::Responder;

    use http::header::{HeaderValue, CONTENT_TYPE};
    use mime_guess::get_mime_type_str;

    /// A marker trait representing that the implementor type implements
    /// both `askama::Template` and `tsukuyomi::output::Responder`.
    ///
    /// The implementation of this trait is automatically derived by the custom Derive.
    pub trait TemplateResponder: Template + Responder + Sealed {}

    pub trait Sealed {}

    /// A helper function to generate an HTTP response from Askama template.
    ///
    /// This function is used by the custom Derive `TemplateResponder` internally.
    pub fn respond(t: &dyn Template, ext: &str) -> tsukuyomi::error::Result<Response<String>> {
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
}
