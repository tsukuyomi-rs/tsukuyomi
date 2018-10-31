//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use derive_more::From;
use futures::Future;
use http::{Method, StatusCode};

use crate::error::{Error, ErrorMessage};
use crate::extractor::{Extract, Extractor};
use crate::input::Input;

macro_rules! define_http_method_extractors {
    ($( $name:ident, $Name:ident => $METHOD:ident; )*) => {$(

        pub fn $name<E>(extractor: E) -> $Name<E>
        where
            E: Extractor,
        {
            $Name(extractor)
        }

        #[derive(Debug, From)]
        pub struct $Name<E>(E);

        impl<E> Extractor for $Name<E>
        where
            E: Extractor,
        {
            type Output = E::Output;
            type Error = Error;
            type Future = futures::future::MapErr<E::Future, fn(E::Error) -> Error>;

            #[inline]
            fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
                if input.method() == Method::$METHOD {
                    match self.0.extract(input).map_err(Into::into)? {
                        Extract::Ready(out) => Ok(Extract::Ready(out)),
                        Extract::Incomplete(fut) => Ok(Extract::Incomplete(fut.map_err(Into::into as fn(E::Error) -> Error))),
                    }
                } else {
                    Err(ErrorMessage::new(StatusCode::METHOD_NOT_ALLOWED, "").into())
                }
            }
        }
    )*};
}

define_http_method_extractors! {
    get, Get => GET;
    post, Post => POST;
    put, Put => PUT;
    delete, Delete => DELETE;
    head, Head => HEAD;
    options, Options => OPTIONS;
    connect, Connect => CONNECT;
    patch, Patch => PATCH;
    trace, Trace => TRACE;
}
