//! The procedural macros for Tsukuyomi.

#![warn(
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![doc(test(no_crate_inject))]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(result_map_unwrap_or_else))]

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

mod derive_responder;
mod route_expr_impl;

use proc_macro::TokenStream;

#[proc_macro]
pub fn route_expr_impl(input: TokenStream) -> TokenStream {
    syn::parse(input)
        .map(|input| crate::route_expr_impl::route_expr_impl(&input))
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

#[proc_macro_derive(Responder, attributes(responder))]
#[allow(nonstandard_style)]
pub fn Responder(input: TokenStream) -> TokenStream {
    syn::parse(input)
        .and_then(crate::derive_responder::parse)
        .map(quote::ToTokens::into_token_stream)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
