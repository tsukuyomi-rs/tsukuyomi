//! Askama integration for Tsukuyomi.
//!
//! ```
//! extern crate askama;
//! extern crate tsukuyomi;
//! extern crate tsukuyomi_askama;
//!
//! use askama::Template;
//! use tsukuyomi::{
//!     app::config::prelude::*,
//!     output::IntoResponse,
//!     App,
//! };
//!
//! #[derive(Template, IntoResponse)]
//! #[template(source = "Hello, {{name}}!", ext = "html")]
//! #[response(with = "tsukuyomi_askama::into_response")]
//! struct Index {
//!     name: String,
//! }
//!
//! # fn main() -> tsukuyomi::app::Result<()> {
//! App::create(
//!     path!(/ {path::param("name")})
//!         .to(endpoint::get().call(|name| Index { name }))
//! )
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
    futures::Poll,
    http::{
        header::{HeaderValue, CONTENT_TYPE},
        Request, Response,
    },
    mime_guess::get_mime_type_str,
    tsukuyomi::{
        error::{internal_server_error, Error},
        handler::{AllowedMethods, Handle, Handler, ModifyHandler},
        input::Input,
        output::IntoResponse,
    },
};

#[inline]
#[allow(clippy::needless_pass_by_value)]
pub fn into_response<T>(t: T, _: &Request<()>) -> tsukuyomi::Result<Response<String>>
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

#[derive(Debug)]
pub struct Rendered<T: Template>(T);

impl<T: Template> IntoResponse for Rendered<T> {
    type Body = String;
    type Error = Error;

    #[inline]
    fn into_response(self, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response(self.0, request)
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
    type Error = H::Error;
    type Handle = RenderedHandle<H::Handle>;

    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        self.inner.allowed_methods()
    }

    fn handle(&self) -> Self::Handle {
        RenderedHandle(self.inner.handle())
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct RenderedHandle<H>(H);

impl<H> Handle for RenderedHandle<H>
where
    H: Handle,
    H::Output: Template,
{
    type Output = Rendered<H::Output>;
    type Error = H::Error;

    #[inline]
    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Output, Self::Error> {
        self.0.poll_ready(input).map(|x| x.map(Rendered))
    }
}
