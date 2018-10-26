//! Components for adding validation of HTTP method.

#![allow(missing_docs)]

use bytes::Bytes;
use http::{Method, StatusCode};
use std::ops::Deref;

use crate::error::{Error, ErrorMessage};
use crate::input::Input;

use super::{FromInput, Preflight};

macro_rules! define_http_method_extractors {
    ($( $Name:ident => $METHOD:ident; )*) => {$(
        #[derive(Debug)]
        pub struct $Name<T>(pub T);

        impl<T> $Name<T>
        where
            T: FromInput,
        {
            #[inline]
            pub fn into_inner(self) -> T {
                self.0
            }
        }

        impl<T> Deref for $Name<T>
        where
            T: FromInput,
        {
            type Target = T;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<T> FromInput for $Name<T>
        where
            T: FromInput,
        {
            type Error = Error;
            type Ctx = T::Ctx;

            fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
                if input.method() == Method::$METHOD {
                    T::preflight(input)
                        .map(|preflight| preflight.map_completed($Name))
                        .map_err(Into::into)
                } else {
                    Err(ErrorMessage::new(StatusCode::METHOD_NOT_ALLOWED, "").into())
                }
            }

            fn finalize(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
                T::finalize(data, input, cx).map($Name).map_err(Into::into)
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
