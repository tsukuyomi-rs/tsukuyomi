extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;

#[allow(non_snake_case)]
#[proc_macro_derive(LocalData)]
pub fn LocalData(input: TokenStream) -> TokenStream {
    let input: syn::DeriveInput = match syn::parse(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error().into(),
    };
    let ident = &input.ident;
    (quote::quote!{
        use tsukuyomi::input::local_map::local_key;
        impl LocalData for #ident {
            local_key!(const KEY: Self);
        }
    }).into()
}
