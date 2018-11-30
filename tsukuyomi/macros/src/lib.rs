//! The procedural macros for Tsukuyomi.

#![recursion_limit = "256"]
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
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;
extern crate tsukuyomi_internal;

mod derive_responder;
mod route_expr_impl;
mod route_impl;
mod validate_prefix;
use tsukuyomi_internal::uri;

use {proc_macro::TokenStream, quote::quote_spanned};

#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
fn to_compile_error(err: syn::parse::Error) -> proc_macro2::TokenStream {
    let message = err.to_string();
    quote_spanned!(err.span() => __tsukuyomi_compile_error!(#message);)
}

#[proc_macro]
#[cfg_attr(tarpaulin, skip)]
pub fn route_expr_impl(input: TokenStream) -> TokenStream {
    crate::route_expr_impl::route_expr_impl(input)
        .unwrap_or_else(to_compile_error)
        .into()
}

#[proc_macro]
#[cfg_attr(tarpaulin, skip)]
pub fn route_impl(input: TokenStream) -> TokenStream {
    crate::route_impl::route_impl(input)
        .unwrap_or_else(to_compile_error)
        .into()
}

#[proc_macro]
#[cfg_attr(tarpaulin, skip)]
pub fn validate_prefix(input: TokenStream) -> TokenStream {
    validate_prefix::validate(input.into())
        .map(|_| quote::quote!(const _DUMMY: () = ();))
        .unwrap_or_else(to_compile_error)
        .into()
}

#[proc_macro_derive(Responder, attributes(responder))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn Responder(input: TokenStream) -> TokenStream {
    crate::derive_responder::derive_responder(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
