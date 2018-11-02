use proc_macro2::TokenStream;
use quote::quote;

#[allow(nonstandard_style)]
pub fn derive(input: &syn::DeriveInput) -> TokenStream {
    let Self_ = &input.ident;

    let Error = quote!(tsukuyomi_toolkit::askama::private::Error);
    let Input = quote!(tsukuyomi_toolkit::askama::private::Input);
    let Responder = quote!(tsukuyomi_toolkit::askama::private::Responder);
    let Response = quote!(tsukuyomi_toolkit::askama::private::Response);
    let Template = quote!(tsukuyomi_toolkit::askama::private::Template);
    let respond = quote!(tsukuyomi_toolkit::askama::respond);

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
