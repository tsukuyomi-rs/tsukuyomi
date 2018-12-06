use {
    crate::{handler::Handle, input::Input, output::Output},
    http::{Method, StatusCode},
    std::fmt,
};

pub use super::router::Resource;

#[derive(Debug)]
pub enum FallbackKind<'a> {
    NotFound(Vec<&'a Resource>),
    FoundResource(&'a Resource),
}

#[derive(Debug)]
pub struct Context<'a> {
    pub input: &'a mut Input<'a>,
    pub kind: &'a FallbackKind<'a>,
    pub(super) _priv: (),
}

/// A trait representing the callback function to be called when the incoming request
/// does not match to the registered routes in the application.
pub trait Fallback: Send + Sync + 'static {
    fn call(&self, cx: &mut Context<'_>) -> Handle;
}

impl<F, R> Fallback for F
where
    F: Fn(&mut Context<'_>) -> R + Send + Sync + 'static,
    R: Into<Handle>,
{
    fn call(&self, cx: &mut Context<'_>) -> Handle {
        (*self)(cx).into()
    }
}

pub(super) struct BoxedFallback {
    inner: Box<dyn Fn(&mut Context<'_>) -> Handle + Send + Sync + 'static>,
}

impl fmt::Debug for BoxedFallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedFallback").finish()
    }
}

impl<F> From<F> for BoxedFallback
where
    F: Fallback,
{
    fn from(fallback: F) -> Self {
        BoxedFallback {
            inner: Box::new(move |cx| fallback.call(cx)),
        }
    }
}

impl BoxedFallback {
    pub(crate) fn call(&self, cx: &mut Context<'_>) -> Handle {
        (self.inner)(cx)
    }
}

/// The default fallback when the `Fallback` is not registered.
pub fn default(cx: &mut Context<'_>) -> Handle {
    match cx.kind {
        FallbackKind::NotFound(..) => Handle::err(StatusCode::NOT_FOUND.into()),
        FallbackKind::FoundResource(resource) => {
            if cx.input.request.method() == Method::HEAD {
                return resource
                    .allowed_methods
                    .get(&Method::GET)
                    .map(|&i| resource.endpoints[i].handler.call(&mut *cx.input))
                    .unwrap_or_else(|| Handle::err(StatusCode::METHOD_NOT_ALLOWED.into()));
            }

            if cx.input.request.method() == Method::OPTIONS {
                let mut response = Output::default();
                response
                    .headers_mut()
                    .insert(http::header::ALLOW, resource.allowed_methods_value.clone());
                return Handle::ok(response);
            }

            Handle::err(StatusCode::METHOD_NOT_ALLOWED.into())
        }
    }
}
