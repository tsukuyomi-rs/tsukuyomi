//! The procedural macros for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-internal-macros/0.4.0-dev")]
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

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

mod route_impl;

use proc_macro::TokenStream;

#[proc_macro]
#[cfg_attr(feature = "cargo-clippy", allow(result_map_unwrap_or_else))]
pub fn route_impl(input: TokenStream) -> TokenStream {
    syn::parse(input)
        .map(|input| crate::route_impl::derive(&input))
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
