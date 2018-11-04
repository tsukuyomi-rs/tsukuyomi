extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(TemplateResponder)]
#[allow(nonstandard_style)]
#[cfg_attr(feature = "cargo-clippy", allow(result_map_unwrap_or_else))]
pub fn TemplateResponder(input: TokenStream) -> TokenStream {
    let input: syn::DeriveInput = match syn::parse(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error().into(),
    };

    let Self_ = &input.ident;

    let Error = quote!(tsukuyomi_askama::private::Error);
    let Input = quote!(tsukuyomi_askama::private::Input);
    let Responder = quote!(tsukuyomi_askama::private::Responder);
    let Response = quote!(tsukuyomi_askama::private::Response);
    let Template = quote!(tsukuyomi_askama::private::Template);
    let TemplateResponder = quote!(tsukuyomi_askama::private::TemplateResponder);
    let Sealed = quote!(tsukuyomi_askama::private::Sealed);
    let respond = quote!(tsukuyomi_askama::private::respond);

    let output = quote!(
        impl #Responder for #Self_ {
            type Body = String;
            type Error = #Error;

            fn respond_to(self, _: &mut #Input<'_>) -> Result<#Response<Self::Body>, Self::Error> {
                use #Template;
                #respond(&self, self.extension().unwrap_or("html"))
            }
        }

        impl #TemplateResponder for #Self_ {}
        impl #Sealed for #Self_ {}
    );

    output.into()
}
