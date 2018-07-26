//! Code generation support for Tsukuyomi.

extern crate proc_macro;
extern crate proc_macro2;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;
#[macro_use]
extern crate failure;

mod async_handler;
mod handler;

use proc_macro::TokenStream;

macro_rules! try_quote {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                use proc_macro2::Span;
                let msg = e.to_string();
                return Into::into(quote_spanned!(Span::call_site() => compile_error!(#msg)));
            }
        }
    };
}

/// Modifies the signature of a free-standing function to a suitable form as handler function.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    try_quote!(handler::handler(attr.into(), item.into())).into()
}

/// Modifies the signature of a free-standing function to a suitable form as handler function.
#[proc_macro_attribute]
pub fn async_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    try_quote!(async_handler::async_handler(attr.into(), item.into())).into()
}
