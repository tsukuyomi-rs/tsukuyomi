use http::{Request, Response};

use crate::error::{Critical, Error};
use crate::output::{Output, ResponseBody};

use super::builder::AppContext;

pub use super::imp::RecognizeError;

/// A trait representing a global error handlers.
pub trait ErrorHandler {
    /// Converts an error value into an HTTP response.
    fn handle_error(&self, err: Error, request: &Request<()>) -> Result<Output, Critical>;
}

impl<F, Bd> ErrorHandler for F
where
    F: Fn(Error, &Request<()>) -> Result<Response<Bd>, Critical>,
    Bd: Into<ResponseBody>,
{
    fn handle_error(&self, err: Error, request: &Request<()>) -> Result<Output, Critical> {
        (*self)(err, request).map(|response| response.map(Into::into))
    }
}

#[allow(missing_debug_implementations)]
pub struct Context<'a> {
    inner: &'a mut AppContext,
}

impl<'a> Context<'a> {
    pub(super) fn new(inner: &'a mut AppContext) -> Self {
        Self { inner }
    }
}

pub trait Global {
    fn configure(self, cx: &mut Context<'_>);
}

impl Global for () {
    fn configure(self, _: &mut Context<'_>) {}
}

pub(super) fn raw<F>(f: F) -> impl Global
where
    F: FnOnce(&mut Context<'_>),
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F> Global for Raw<F>
    where
        F: FnOnce(&mut Context<'_>),
    {
        fn configure(self, cx: &mut Context<'_>) {
            (self.0)(cx)
        }
    }

    Raw(f)
}

#[derive(Debug)]
pub struct Builder<G: Global = ()> {
    config: G,
}

impl Default for Builder {
    fn default() -> Self {
        Self { config: () }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<G> Builder<G>
where
    G: Global,
{
    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(self, enabled: bool) -> Builder<impl Global> {
        Builder {
            config: raw(move |cx| {
                self.config.configure(cx);
                cx.inner.fallback_head(enabled);
            }),
        }
    }

    /// Specifies whether to use the default `OPTIONS` handlers if it is not registered.
    ///
    /// If `enabled`, it creates the default OPTIONS handlers by collecting the registered
    /// methods from the router and then adds them to the *global* scope.
    pub fn fallback_options(self, enabled: bool) -> Builder<impl Global> {
        Builder {
            config: raw(move |cx| {
                self.config.configure(cx);
                cx.inner.fallback_options(enabled);
            }),
        }
    }

    /// Sets the instance to an error handler into this builder.
    pub fn error_handler(
        self,
        error_handler: impl ErrorHandler + Send + Sync + 'static,
    ) -> Builder<impl Global> {
        Builder {
            config: raw(move |cx| {
                self.config.configure(cx);
                cx.inner.set_error_handler(error_handler);
            }),
        }
    }
}

impl<G> Global for Builder<G>
where
    G: Global,
{
    #[inline]
    fn configure(self, cx: &mut Context<'_>) {
        self.config.configure(cx)
    }
}
