extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;
extern crate synstructure;

#[macro_use]
mod util;
//mod extract;
mod local_data;

//use crate::extract::HandleMode;
use synstructure::decl_derive;

decl_derive! {
    [LocalData] => crate::local_data::derive_local_data
}

/*
decl_attribute! {
    /// A macro for creating handler function.
    fn extract_ready(item: syn::ItemFn) -> syn::ItemFn {
        crate::extract::derive_handler(item, HandleMode::Ready)
    }
}

decl_attribute! {
    /// A macro for creating handler function.
    fn extract(item: syn::ItemFn) -> syn::ItemFn {
        crate::extract::derive_handler(item, HandleMode::Polling)
    }
}
*/
