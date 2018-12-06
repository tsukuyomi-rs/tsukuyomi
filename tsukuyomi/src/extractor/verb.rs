//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use {
    super::Extractor,
    crate::{
        error::Error,
        future::{Future, MaybeFuture},
        input::Input,
    },
    http::Method,
};

pub fn verb<E>(extractor: E, method: Method) -> impl Extractor<Output = E::Output>
where
    E: Extractor,
{
    #[allow(missing_debug_implementations)]
    struct Wrapped<E>(E, Method);

    #[allow(clippy::type_complexity)]
    impl<E> Extractor for Wrapped<E>
    where
        E: Extractor,
    {
        type Output = E::Output;
        type Future = crate::future::MapErr<E::Future, fn(<E::Future as Future>::Error) -> Error>;

        #[inline]
        fn extract(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            if input.request.method() != self.1 {
                return MaybeFuture::err(crate::error::method_not_allowed("rejected by extractor"));
            }
            self.0.extract(input).map_err(Into::into)
        }
    }

    Wrapped(extractor, method)
}

macro_rules! define_http_method_extractors {
    ($( $name:ident => $METHOD:ident; )*) => {$(
        pub fn $name<E>(extractor: E) -> impl Extractor<Output = E::Output>
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
