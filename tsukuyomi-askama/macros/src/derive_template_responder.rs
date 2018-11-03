use proc_macro2::TokenStream;
use quote::quote;

#[allow(nonstandard_style)]
pub fn derive(input: &syn::DeriveInput) -> TokenStream {
    let Self_ = &input.ident;

    let Error = quote!(tsukuyomi_askama::private::Error);
    let Input = quote!(tsukuyomi_askama::private::Input);
    let Responder = quote!(tsukuyomi_askama::private::Responder);
    let Response = quote!(tsukuyomi_askama::private::Response);
    let Template = quote!(tsukuyomi_askama::private::Template);
    let respond = quote!(tsukuyomi_askama::respond);

    quote!(
        impl #Responder for #Self_ {
            type Body = String;
            type Error = #Error;

            fn respond_to(self, _: &mut #Input<'_>) -> Result<#Response<Self::Body>, Self::Error> {
                use #Template;
                #respond(&self, self.extension().unwrap_or("html"))
            }
        }
    )
}
