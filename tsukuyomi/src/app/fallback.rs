use {
    super::Resource,
    crate::{handler::Handle, input::Input, output::Output},
    http::{Method, StatusCode},
};

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

/// The default fallback when the `Fallback` is not registered.
pub fn default(cx: &mut Context<'_>) -> Handle {
    match cx.kind {
        FallbackKind::NotFound(..) => Handle::err(StatusCode::NOT_FOUND.into()),
        FallbackKind::FoundResource(resource) => {
            if cx.input.request.method() == Method::HEAD {
                if resource.allowed_methods.contains(&Method::GET) {
                    return Handle::err(StatusCode::METHOD_NOT_ALLOWED.into());
                }
                return resource.handler.call(&mut *cx.input);
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
