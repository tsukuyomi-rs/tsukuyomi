//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use {
    super::Extractor,
    crate::{common::MaybeFuture, error::Error, input::Input},
    futures::Future,
    http::Method,
};

pub fn verb<E>(extractor: E, method: Method) -> impl Extractor<Output = E::Output, Error = Error>
where
    E: Extractor,
{
    #[allow(missing_debug_implementations)]
    struct Wrapped<E>(E, Method);

    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    impl<E> Extractor for Wrapped<E>
    where
        E: Extractor,
    {
        type Output = E::Output;
        type Error = Error;
        type Future = futures::future::MapErr<E::Future, fn(E::Error) -> Error>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            if input.request.method() != self.1 {
                return MaybeFuture::err(crate::error::method_not_allowed("rejected by extractor"));
            }
            match self.0.extract(input) {
                MaybeFuture::Ready(result) => MaybeFuture::Ready(result.map_err(Into::into)),
                MaybeFuture::Future(future) => {
                    MaybeFuture::from(future.map_err(Into::into as fn(E::Error) -> Error))
                }
            }
        }
    }

    Wrapped(extractor, method)
}

macro_rules! define_http_method_extractors {
    ($( $name:ident => $METHOD:ident; )*) => {$(
        pub fn $name<E>(extractor: E) -> impl Extractor<Output = E::Output, Error = Error>
        where
            E: Extractor,
        {
            self::verb(extractor, Method::$METHOD)
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
