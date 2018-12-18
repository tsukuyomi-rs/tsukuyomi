//! Extractors for validation of HTTP method.

use {
    super::Extractor,
    crate::future::TryFuture,
    http::{Method, StatusCode},
};

/// Creates an `Extractor` that checks if the request method is equal to `method`.
///
/// If the method is incorrect, it returns `Err(StatusCode::METHOD_NOT_ALLOWED)`.
pub fn equals(
    method: Method,
) -> impl Extractor<
    Output = (), //
    Error = StatusCode,
    Extract = impl TryFuture<Ok = (), Error = StatusCode> + Send + 'static,
> {
    super::ready(move |input| {
        if input.request.method() == method {
            Ok(())
        } else {
            Err(StatusCode::METHOD_NOT_ALLOWED)
        }
    })
}

macro_rules! define_http_method_extractors {
    ($( $name:ident => $METHOD:ident; )*) => {$(
        pub fn $name() -> impl Extractor<
            Output = (),
            Error = StatusCode,
            Extract = impl TryFuture<Ok = (), Error = StatusCode> + Send + 'static,
        > {
            self::equals(Method::$METHOD)
        }
    )*};
}

define_http_method_extractors! {
    get => GET;
    post => POST;
    put => PUT;
    delete => DELETE;
    head => HEAD;
    options => OPTIONS;
    connect => CONNECT;
    patch => PATCH;
    trace => TRACE;
}

pub fn get_or_head() -> impl Extractor<
    Output = (),
    Error = StatusCode,
    Extract = impl TryFuture<Ok = (), Error = StatusCode> + Send + 'static,
> {
    super::ready(move |input| {
        if input.request.method() == Method::GET || input.request.method() == Method::HEAD {
            Ok(())
        } else {
            Err(StatusCode::METHOD_NOT_ALLOWED)
        }
    })
}
