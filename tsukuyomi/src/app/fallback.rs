use {
    super::{AppData, Resource},
    crate::{error::Error, output::Output},
    http::{Method, Request, StatusCode},
};

/// A trait representing the callback function to be called when the incoming request
/// does not match to the registered routes in the application.
pub trait Fallback {
    fn call(&self, cx: &Context<'_>) -> Result<Output, Error>;
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
    pub(super) request: &'a Request<()>,
    pub(super) app: &'a AppData,
    pub(super) resource: Option<&'a Resource>,
}

impl<'a> Context<'a> {
    pub fn request(&self) -> &Request<()> {
        &*self.request
    }

    pub fn is_no_route(&self) -> bool {
        self.resource.is_none()
    }

    pub fn methods(&self) -> Option<impl Iterator<Item = &'a Method> + 'a> {
        Some(self.resource?.route_ids.keys())
    }
}

#[allow(missing_debug_implementations)]
pub(super) struct FallbackInstance(Box<dyn Fallback + Send + Sync + 'static>);

impl<F> From<F> for FallbackInstance
where
    F: Fallback + Send + Sync + 'static,
{
    fn from(fallback: F) -> Self {
        FallbackInstance(Box::new(fallback))
    }
}

impl std::ops::Deref for FallbackInstance {
    type Target = dyn Fallback + Send + Sync + 'static;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The default fallback when the `Fallback` is not registered.
pub fn default(cx: &Context<'_>) -> Result<Output, Error> {
    let resoruce = cx.resource.ok_or_else(|| StatusCode::NOT_FOUND)?;

    if cx.request.method() != Method::OPTIONS {
        return Err(StatusCode::METHOD_NOT_ALLOWED.into());
    }

    let mut response = Output::default();
    response
        .headers_mut()
        .insert(http::header::ALLOW, resoruce.allowed_methods_value.clone());
    Ok(response)
}
