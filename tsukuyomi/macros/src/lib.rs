//! The procedural macros for Tsukuyomi.

#![recursion_limit = "256"]
#![deny(nonstandard_style, rust_2018_idioms, rust_2018_compatibility, unused)]
#![cfg_attr(test, deny(warnings))]
#![forbid(clippy::unimplemented)]
#![doc(test(attr(deny(deprecated))))]

extern crate proc_macro;

mod derive_responder;
mod path_impl;

use proc_macro::TokenStream;

/// A procedural macro for deriving the implementation of `IntoResponse`.
///
/// See the documentation of the trait `IntoResponse` for details.
#[proc_macro_derive(Responder, attributes(response))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn IntoResponse(input: TokenStream) -> TokenStream {
    crate::derive_responder::derive(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

#[proc_macro]
pub fn path_impl(input: TokenStream) -> TokenStream {
    crate::path_impl::path_impl(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
