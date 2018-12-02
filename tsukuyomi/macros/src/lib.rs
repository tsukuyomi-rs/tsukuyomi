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

mod derive_responder;

use proc_macro::TokenStream;

#[proc_macro_derive(Responder, attributes(responder))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn Responder(input: TokenStream) -> TokenStream {
    crate::derive_responder::derive_responder(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
