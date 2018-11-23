use {
    super::{AppData, EndpointData},
    crate::{error::Error, output::Output},
    http::{Method, Request, StatusCode},
};

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
    pub(super) endpoint: Option<&'a EndpointData>,
}

impl<'a> Context<'a> {
    pub fn request(&self) -> &Request<()> {
        &*self.request
    }

    pub fn is_no_route(&self) -> bool {
        self.endpoint.is_none()
    }

    pub fn methods(&self) -> Option<impl Iterator<Item = &'a Method> + 'a> {
        Some(self.endpoint?.route_ids.keys())
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

pub fn default(cx: &Context<'_>) -> Result<Output, Error> {
    if cx.endpoint.is_none() {
        return Err(StatusCode::NOT_FOUND.into());
    }

    if cx.request.method() != Method::OPTIONS {
        return Err(StatusCode::METHOD_NOT_ALLOWED.into());
    }

    let allowed_methods = cx.endpoint.unwrap().allowed_methods_value.clone();
    let mut response = Output::default();
    response
        .headers_mut()
        .insert(http::header::ALLOW, allowed_methods);
    Ok(response)
}
