#![allow(deprecated)]
#![deprecated(
    since = "0.4.2",
    note = "the module `app::route` will be removed in the next version."
)]

#[doc(hidden)]
pub use http::Method;
use {
    crate::{handler::Handler, uri::Uri},
    indexmap::IndexSet,
};

pub use super::scope::Route as Builder;

/// A trait representing the types for constructing a route in `App`.
pub trait Route {
    type Error: Into<super::Error>;

    fn configure(self, cx: &mut Context) -> Result<(), Self::Error>;
}

#[deprecated(
    since = "0.4.2",
    note = "the trait Route will be removed in the next version."
)]
#[allow(missing_debug_implementations)]
pub struct Context {
    pub(super) uri: Uri,
    pub(super) methods: Option<IndexSet<Method>>,
    pub(super) handler: Option<Box<dyn Handler + Send + Sync + 'static>>,
}

impl Context {
    pub(super) fn uri(&mut self, uri: Uri) {
        self.uri = uri;
    }

    pub(super) fn methods<I>(&mut self, methods: I)
    where
        I: IntoIterator<Item = Method>,
    {
        self.methods = Some(methods.into_iter().collect());
    }

    pub(super) fn handler<H>(&mut self, handler: H)
    where
        H: Handler + Send + Sync + 'static,
    {
        self.handler = Some(Box::new(handler));
    }
}
