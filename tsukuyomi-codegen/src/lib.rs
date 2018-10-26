extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;
extern crate synstructure;

use proc_macro::TokenStream;

mod handler;
mod local_data;

synstructure::decl_derive!([LocalData] => crate::local_data::derive_local_data);

/// A macro for creating handler function.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mode: crate::handler::HandlerMode = match attr.to_string().parse() {
        Ok(mode) => mode,
        Err(message) => {
            return syn::parse::Error::new(proc_macro2::Span::call_site(), message)
                .to_compile_error()
                .into()
        }
    };

    let item: syn::ItemFn = match syn::parse(item) {
        Ok(item) => item,
        Err(err) => return err.to_compile_error().into(),
    };

    crate::handler::handler(item, mode).into()
}
