use proc_macro2::{Span, TokenStream};
use quote::*;

mod parsing {
    use syn::parse::{Parse, ParseStream, Result};

    #[derive(Debug)]
    pub struct RouteInput {
        pub method: syn::Ident,
        pub uri: syn::LitStr,
        _priv: (),
    }

    impl Parse for RouteInput {
        fn parse(input: ParseStream<'_>) -> Result<Self> {
            Ok(Self {
                method: input.parse()?,
                uri: input.parse()?,
                _priv: (),
            })
        }
    }
}

#[allow(nonstandard_style)]
pub fn derive(input: &parsing::RouteInput) -> TokenStream {
    enum ParamKind {
        Pos(usize),
        Wildcard,
    }

    let mut params = vec![];
    for segment in input.uri.value().split('/') {
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
    let method = &input.method;
    let uri = &input.uri;
    let Extractor = quote!(tsukuyomi::extractor::Extractor);
    let Route = quote!(tsukuyomi::route::Route);
    let route = quote!(tsukuyomi::route);

    if params.is_empty() {
        quote! {
            fn #name() -> #Route<impl #Extractor<Output = ()>> {
                #route::#method(#uri)
            }
        }
    } else {
        let type_params = params.iter().map(|(ty, _)| ty);
        let return_types = params.iter().map(|(ty, _)| ty);

        let bounds = params.iter().map(|(ty, _)| {
            quote!(
                #ty: std::str::FromStr + Send + 'static,
                <#ty as std::str::FromStr>::Err: std::fmt::Debug + std::fmt::Display + Send + 'static,
            )
        });

        let extractors = params.iter().map(|(_, kind)| -> syn::Expr {
            match kind {
                ParamKind::Pos(i) => syn::parse_quote!(tsukuyomi::extractor::param::pos(#i)),
                ParamKind::Wildcard => syn::parse_quote!(tsukuyomi::extractor::param::wildcard()),
            }
        });

        quote!(
            fn #name<#(#type_params),*>()
                -> #Route<impl #Extractor<Output = (#(#return_types,)*)>>
            where
                #( #bounds )*
            {
                #route::#method(#uri)
                    #( .with(#extractors) )*
            }
        )
    }
}
