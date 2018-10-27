//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use bytes::Bytes;
use derive_more::From;
use http::{Method, StatusCode};

use crate::error::{Error, ErrorMessage};
use crate::input::Input;

use super::extractor::{Extractor, Preflight};

macro_rules! define_http_method_extractors {
    ($( $Name:ident => $METHOD:ident; )*) => {$(
        #[derive(Debug, From)]
        pub struct $Name<E>(E);

        impl<E> Extractor for $Name<E>
        where
            E: Extractor,
        {
            type Out = E::Out;
            type Error = Error;
            type Ctx = E::Ctx;

            fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
                if input.method() == Method::$METHOD {
                    self.0.preflight(input).map(Preflight::conform).map_err(Into::into)
                } else {
                    Err(ErrorMessage::new(StatusCode::METHOD_NOT_ALLOWED, "").into())
                }
            }

            #[inline]
            fn finalize(cx: Self::Ctx, input: &mut Input<'_>, data: &Bytes) -> Result<Self::Out, Self::Error> {
                E::finalize(cx, input, data).map_err(Into::into)
            }
        }
    )*};
}

define_http_method_extractors! {
    Get => GET;
    Post => POST;
    Put => PUT;
    Delete => DELETE;
    Head => HEAD;
    Options => OPTIONS;
    Connect => CONNECT;
    Patch => PATCH;
    Trace => TRACE;
}
