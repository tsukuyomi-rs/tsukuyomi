use {
    super::router::Resource,
    crate::{handler::AsyncResult, input::Input, output::Output},
    http::{Method, StatusCode},
    std::fmt,
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
    type Handle: Into<Box<dyn AsyncResult<Output> + Send + 'static>>;

    fn call(&self, cx: &mut Context<'_>) -> Self::Handle;
}

impl<F, R> Fallback for F
where
    F: Fn(&mut Context<'_>) -> R + Send + Sync + 'static,
    R: Into<Box<dyn AsyncResult<Output> + Send + 'static>>,
{
    type Handle = R;

    fn call(&self, cx: &mut Context<'_>) -> Self::Handle {
        (*self)(cx)
    }
}

pub struct BoxedFallback(
    Box<
        dyn Fn(&mut Context<'_>) -> Box<dyn AsyncResult<Output> + Send + 'static>
            + Send
            + Sync
            + 'static,
    >,
);

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
        BoxedFallback(Box::new(move |cx| fallback.call(cx).into()))
    }
}

impl BoxedFallback {
    pub(crate) fn call(
        &self,
        cx: &mut Context<'_>,
    ) -> Box<dyn AsyncResult<Output> + Send + 'static> {
        (self.0)(cx)
    }
}

/// The default fallback when the `Fallback` is not registered.
pub fn default(cx: &mut Context<'_>) -> Box<dyn AsyncResult<Output> + Send + 'static> {
    match cx.kind {
        FallbackKind::NotFound(..) => Box::new(crate::handler::err(StatusCode::NOT_FOUND.into())),
        FallbackKind::FoundResource(resource) => {
            if cx.input.request.method() == Method::HEAD {
                return resource
                    .allowed_methods
                    .get(&Method::GET)
                    .map(|&i| resource.endpoints[i].handler.handle(&mut *cx.input))
                    .unwrap_or_else(|| {
                        Box::new(crate::handler::err(StatusCode::METHOD_NOT_ALLOWED.into()))
                    });
            }

            if cx.input.request.method() == Method::OPTIONS {
                let mut response = Output::default();
                response
                    .headers_mut()
                    .insert(http::header::ALLOW, resource.allowed_methods_value.clone());
                return Box::new(crate::handler::ok(response));
            }

            Box::new(crate::handler::err(StatusCode::METHOD_NOT_ALLOWED.into()))
        }
    }
}
