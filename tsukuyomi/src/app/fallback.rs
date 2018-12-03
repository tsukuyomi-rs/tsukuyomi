use {
    super::{router::Resource, AppInner},
    crate::{error::Error, output::Output},
    http::{Method, Request, StatusCode},
};

/// A trait representing the callback function to be called when the incoming request
/// does not match to the registered routes in the application.
pub trait Fallback {
    fn call(&self, cx: &Context<'_>) -> Result<Output, Error>;
}

impl Fallback for () {
    fn call(&self, cx: &Context<'_>) -> Result<Output, Error> {
        self::default(cx)
    }
}

impl<F> Fallback for F
where
    F: Fn(&Context<'_>) -> Result<Output, Error>,
{
    fn call(&self, cx: &Context<'_>) -> Result<Output, Error> {
        (*self)(cx)
    }
}

#[derive(Debug)]
pub struct Context<'a> {
    pub(super) inner: &'a AppInner,
    pub(super) request: &'a Request<()>,
    pub(super) resource: &'a Resource,
}

impl<'a> Context<'a> {
    pub fn request(&self) -> &Request<()> {
        &*self.request
    }

    pub fn methods(&self) -> impl Iterator<Item = &'a Method> + 'a {
        self.resource.allowed_methods.keys()
    }
}

/// The default fallback when the `Fallback` is not registered.
pub fn default(cx: &Context<'_>) -> Result<Output, Error> {
    if cx.request.method() != Method::OPTIONS {
        return Err(StatusCode::METHOD_NOT_ALLOWED.into());
    }

    let mut response = Output::default();
    response.headers_mut().insert(
        http::header::ALLOW,
        cx.resource.allowed_methods_value.clone(),
    );
    Ok(response)
}
