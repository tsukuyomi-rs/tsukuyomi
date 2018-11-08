//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use futures::Future;
use http::Method;

use crate::error::Error;
use crate::extractor::Extractor;
use crate::input::Input;

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
        fn extract(&self, input: &mut Input<'_>) -> Result<Self::Future, Self::Error> {
            if input.method() == self.1 {
                match self.0.extract(input).map_err(Into::into)? {
                    fut => Ok(fut.map_err(Into::into as fn(E::Error) -> Error)),
                }
            } else {
                Err(crate::error::method_not_allowed("rejected by extractor"))
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
