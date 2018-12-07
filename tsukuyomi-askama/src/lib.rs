//! Askama integration for Tsukuyomi.
//!
//! ```
//! extern crate askama;
//! extern crate tsukuyomi;
//! extern crate tsukuyomi_askama;
//!
//! use askama::Template;
//! use tsukuyomi::{
//!     app::{App, route},
//!     output::Responder,
//! };
//!
//! #[derive(Template, Responder)]
//! #[template(source = "Hello, {{name}}!", ext = "html")]
//! #[responder(respond_to = "tsukuyomi_askama::respond_to")]
//! struct Index {
//!     name: String,
//! }
//!
//! # fn main() -> tsukuyomi::app::Result<()> {
//! App::builder()
//!     .with(
//!         route::root()
//!             .param("name")?
//!             .reply(|name| Index { name })
//!     )
//!     .build()
//! #   .map(drop)
//! # }
//! ```

#![doc(html_root_url = "https://docs.rs/tsukuyomi-askama/0.2.0-dev")]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use {
    askama::Template,
    derive_more::From,
    http::{
        header::{HeaderValue, CONTENT_TYPE},
        Response,
    },
    mime_guess::get_mime_type_str,
    tsukuyomi::{
        error::{internal_server_error, Error},
        future::MaybeFuture,
        handler::{Handler, ModifyHandler},
        input::Input,
        output::Responder,
    },
};

#[inline]
#[allow(clippy::needless_pass_by_value)]
pub fn respond_to<T>(t: T, _: &mut Input<'_>) -> tsukuyomi::Result<Response<String>>
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

#[derive(Debug, From)]
pub struct Rendered<T: Template>(T);

impl<T: Template> Responder for Rendered<T> {
    type Body = String;
    type Error = Error;

    #[inline]
    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        self::respond_to(self.0, input)
    }
}

#[derive(Debug, Default)]
pub struct Renderer(());

impl<H> ModifyHandler<H> for Renderer
where
    H: Handler,
    H::Output: Template,
{
    type Output = Rendered<H::Output>;
    type Handler = RenderedHandler<H>;

    fn modify(&self, inner: H) -> Self::Handler {
        RenderedHandler { inner }
    }
}

#[derive(Debug)]
pub struct RenderedHandler<H> {
    inner: H,
}

#[allow(clippy::type_complexity)]
impl<H> Handler for RenderedHandler<H>
where
    H: Handler,
    H::Output: Template,
{
    type Output = Rendered<H::Output>;
    type Future = tsukuyomi::future::MapOk<H::Future, fn(H::Output) -> Rendered<H::Output>>;

    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        self.inner.call(input).map_ok(Rendered::from)
    }
}
