//! Askama integration for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-askama/0.3.0-dev")]
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
    http::{
        header::{HeaderValue, CONTENT_LENGTH, CONTENT_TYPE},
        Response,
    },
    mime_guess::get_mime_type_str,
};

/// An implementor of `Preset` for deriving the implementation of `IntoResponse`
/// to Askama templates.
///
/// # Example
///
/// ```
/// use askama::Template;
/// use tsukuyomi::Responder;
///
/// #[derive(Template, Responder)]
/// #[template(source = "Hello, {{name}}!", ext = "html")]
/// #[response(preset = "tsukuyomi_askama::Askama")]
/// struct Index {
///     name: String,
/// }
/// # fn main() {}
/// ```
#[allow(missing_debug_implementations)]
pub struct Askama(());

mod preset {
    use super::*;
    use tsukuyomi::{
        future::{Poll, TryFuture},
        input::Input,
        output::preset::Preset,
        upgrade::NeverUpgrade,
    };

    impl<T> Preset<T> for Askama
    where
        T: Template,
    {
        type Upgrade = NeverUpgrade;
        type Error = tsukuyomi::Error;
        type Respond = AskamaRespond<T>;

        fn respond(this: T) -> AskamaRespond<T> {
            AskamaRespond(this)
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct AskamaRespond<T>(T);

    impl<T> TryFuture for AskamaRespond<T>
    where
        T: Template,
    {
        type Ok = Response<String>;
        type Error = tsukuyomi::Error;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            Ok(to_response(&self.0)?.into())
        }
    }
}

#[inline]
#[allow(clippy::needless_pass_by_value)]
fn to_response<T>(t: &T) -> tsukuyomi::Result<Response<String>>
where
    T: Template,
{
    let body = t
        .render()
        .map_err(tsukuyomi::error::internal_server_error)?;
    let len = body.len();

    let mut response = Response::new(body);

    let content_type = t
        .extension()
        .and_then(get_mime_type_str)
        .unwrap_or("text/html; charset=utf-8");

    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));

    response.headers_mut().insert(
        CONTENT_LENGTH,
        len.to_string().parse().expect("valid header value"),
    );

    Ok(response)
}

/// Creates a `ModifyHandler` that renders the outputs of handlers as Askama template.
pub fn renderer() -> Renderer {
    Renderer::default()
}

#[derive(Debug, Default)]
pub struct Renderer(());

mod renderer {
    use {
        super::Renderer,
        askama::Template,
        http::Response,
        tsukuyomi::{
            error::Error,
            future::{Poll, TryFuture},
            handler::{metadata::Metadata, Handler, ModifyHandler},
            input::Input,
        },
    };

    impl<H> ModifyHandler<H> for Renderer
    where
        H: Handler,
        H::Output: Template,
    {
        type Output = Response<String>;
        type Error = Error;
        type Handler = RenderedHandler<H>; // private

        fn modify(&self, inner: H) -> Self::Handler {
            RenderedHandler { inner }
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct RenderedHandler<H> {
        inner: H,
    }

    impl<H> Handler for RenderedHandler<H>
    where
        H: Handler,
        H::Output: Template,
    {
        type Output = Response<String>;
        type Error = Error;
        type Handle = RenderedHandle<H::Handle>;

        fn metadata(&self) -> Metadata {
            self.inner.metadata()
        }

        fn handle(&self) -> Self::Handle {
            RenderedHandle(self.inner.handle())
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct RenderedHandle<H>(H);

    impl<H> TryFuture for RenderedHandle<H>
    where
        H: TryFuture,
        H::Ok: Template,
    {
        type Ok = Response<String>;
        type Error = Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let ctx = tsukuyomi::future::try_ready!(self.0.poll_ready(input).map_err(Into::into));
            let response = super::to_response(&ctx)?;
            Ok(response.into())
        }
    }
}
