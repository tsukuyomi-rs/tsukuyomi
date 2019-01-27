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
        header::{HeaderValue, CONTENT_TYPE},
        Request, Response,
    },
    mime_guess::get_mime_type_str,
    tsukuyomi::{
        error::internal_server_error,
        handler::{Handler, ModifyHandler},
        output::preset::Preset,
    },
};

/// An implementor of `Preset` for deriving the implementation of `IntoResponse`
/// to Askama templates.
///
/// # Example
///
/// ```
/// use askama::Template;
/// use tsukuyomi::IntoResponse;
///
/// #[derive(Template, IntoResponse)]
/// #[template(source = "Hello, {{name}}!", ext = "html")]
/// #[response(preset = "tsukuyomi_askama::Askama")]
/// struct Index {
///     name: String,
/// }
/// # fn main() {}
/// ```
#[allow(missing_debug_implementations)]
pub struct Askama(());

impl<T> Preset<T> for Askama
where
    T: Template,
{
    type Body = String;
    type Error = tsukuyomi::Error;

    #[inline]
    fn into_response(ctx: T, request: &Request<()>) -> Result<Response<Self::Body>, Self::Error> {
        self::into_response(ctx, request)
    }
}

#[inline]
#[allow(clippy::needless_pass_by_value)]
fn into_response<T>(t: T, _: &Request<()>) -> tsukuyomi::Result<Response<String>>
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

/// Creates a `ModifyHandler` that renders the outputs of handlers as Askama template.
pub fn renderer() -> Renderer {
    Renderer::default()
}

#[derive(Debug, Default)]
pub struct Renderer(());

impl<H> ModifyHandler<H> for Renderer
where
    H: Handler,
    H::Output: Template,
{
    type Output = Response<String>;
    type Handler = self::renderer::RenderedHandler<H>; // private

    fn modify(&self, inner: H) -> Self::Handler {
        self::renderer::RenderedHandler { inner }
    }
}

mod renderer {
    use {
        askama::Template,
        http::Response,
        tsukuyomi::{
            error::Error,
            future::{Poll, TryFuture},
            handler::{Handler, Metadata},
            input::Input,
        },
    };

    #[allow(missing_debug_implementations)]
    pub struct RenderedHandler<H> {
        pub(super) inner: H,
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
            super::into_response(ctx, input.request).map(Into::into)
        }
    }
}
