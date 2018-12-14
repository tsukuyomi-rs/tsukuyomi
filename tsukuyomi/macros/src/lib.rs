//! The procedural macros for Tsukuyomi.

#![recursion_limit = "256"]
#![deny(nonstandard_style, rust_2018_idioms, rust_2018_compatibility, unused)]
#![forbid(clippy::unimplemented)]

extern crate proc_macro;

mod derive_into_response;

use proc_macro::TokenStream;

/// A procedural macro for deriving the implementation of `IntoResponse`.
#[proc_macro_derive(IntoResponse, attributes(response))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn IntoResponse(input: TokenStream) -> TokenStream {
    crate::derive_into_response::derive_into_response(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
