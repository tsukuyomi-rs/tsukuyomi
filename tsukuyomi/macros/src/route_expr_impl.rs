use proc_macro2::{Span, TokenStream};
use quote::*;

#[allow(nonstandard_style)]
pub fn route_expr_impl(uri: &syn::LitStr) -> TokenStream {
    enum ParamKind {
        Pos(usize),
        Wildcard,
    }

    let mut params = vec![];
    for segment in uri.value().split('/') {
        match segment.as_bytes().get(0) {
            Some(b':') => {
                let i = params.len();
                let ident = syn::Ident::new(&format!("T{}", i), Span::call_site());
                params.push((ident, ParamKind::Pos(i)));
            }
            Some(b'*') => {
                let i = params.len();
                let ident = syn::Ident::new(&format!("T{}", i), Span::call_site());
                params.push((ident, ParamKind::Wildcard));
            }
            _ => {}
        }
    }

    let name = quote!(route);
    let Extractor = quote!(tsukuyomi::extractor::Extractor);
    let FromParam = quote!(tsukuyomi::extractor::param::FromParam);
    let Builder = quote!(tsukuyomi::route::Builder);
    let route = quote!(tsukuyomi::route::route);

    if params.is_empty() {
        quote! {
            fn #name() -> #Builder<impl #Extractor<Output = ()>> {
                #route(#uri)
            }
        }
    } else {
        let type_params = params.iter().map(|(ty, _)| ty);
        let return_types = params.iter().map(|(ty, _)| ty);
        let bounds = params.iter().map(|(ty, _)| quote!(#ty: #FromParam,));
        let extractors = params.iter().map(|(_, kind)| -> syn::Expr {
            match kind {
                ParamKind::Pos(i) => syn::parse_quote!(tsukuyomi::extractor::param::pos(#i)),
                ParamKind::Wildcard => syn::parse_quote!(tsukuyomi::extractor::param::wildcard()),
            }
        });

        quote!(
            fn #name<#(#type_params),*>()
                -> #Builder<impl #Extractor<Output = (#(#return_types,)*)>>
            where
                #( #bounds )*
            {
                #route(#uri)
                    #( .with(#extractors) )*
            }
        )
    }
}
