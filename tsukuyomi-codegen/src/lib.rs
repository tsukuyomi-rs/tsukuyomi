extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;
extern crate synstructure;

#[macro_use]
mod util;
mod handler;
mod local_data;

use crate::handler::HandlerMode;
use synstructure::decl_derive;

decl_derive! {
    [LocalData] => crate::local_data::derive_local_data
}

decl_attribute! {
    /// A macro for creating handler function.
    fn handler(item: syn::ItemFn) -> syn::ItemFn {
        crate::handler::derive_handler(item, HandlerMode::Auto)
    }
}

decl_attribute! {
    /// A macro for creating handler function.
    fn future_handler(item: syn::ItemFn) -> syn::ItemFn {
        crate::handler::derive_handler(item, HandlerMode::Future)
    }
}
