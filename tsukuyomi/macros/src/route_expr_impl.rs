use proc_macro2::{Span, TokenStream};
use quote::*;

pub fn route_expr_impl(input: impl Into<TokenStream>) -> syn::parse::Result<TokenStream> {
    parse(input.into()).map(|input| derive(&input))
}

enum ParamKind {
    Pos(usize),
    Wildcard,
}

struct RouteExprImplInput {
    uri: syn::LitStr,
    params: Vec<(syn::Ident, ParamKind)>,
}

fn parse(input: TokenStream) -> syn::parse::Result<RouteExprImplInput> {
    let uri: syn::LitStr = syn::parse2(input)?;

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

    Ok(RouteExprImplInput { uri, params })
}

#[allow(nonstandard_style)]
fn derive(input: &RouteExprImplInput) -> TokenStream {
    let name = quote!(route);
    let Extractor = quote!(tsukuyomi::extractor::Extractor);
    let FromParam = quote!(tsukuyomi::extractor::param::FromParam);
    let Error = quote!(tsukuyomi::error::Error);
    let route = quote!(tsukuyomi::app::route);
    let Builder = quote!(tsukuyomi::app::route::Builder);
    let uri = &input.uri;

    if input.params.is_empty() {
        quote! {
            fn #name() -> #Builder<()> {
                #route()
                    .uri(#uri.parse().expect("this is a bug"))
            }
        }
    } else {
        let type_params = input.params.iter().map(|(ty, _)| ty);
        let return_types = input.params.iter().map(|(ty, _)| ty);
        let bounds = input.params.iter().map(|(ty, _)| quote!(#ty: #FromParam,));
        let extractors = input.params.iter().map(|(_, kind)| -> syn::Expr {
            match kind {
                ParamKind::Pos(i) => syn::parse_quote!(tsukuyomi::extractor::param::pos(#i)),
                ParamKind::Wildcard => syn::parse_quote!(tsukuyomi::extractor::param::wildcard()),
            }
        });

        quote!(
            fn #name<#(#type_params),*>() -> #Builder<
                impl #Extractor<Output = (#(#return_types,)*), Error = #Error>,
            >
            where
                #( #bounds )*
            {
                #route()
                    .uri(#uri.parse().expect("this is a bug"))
                    #( .with(#extractors) )*
            }
        )
    }
}

macro_rules! t {
    (
        name: $name:ident,
        source: ($($source:tt)*),
        expected: { $($expected:tt)* },
    ) => {
        #[test]
        fn $name() {
            match route_expr_impl(quote!($($source)*)) {
                Ok(output) => assert_eq!(quote!(#output).to_string(), quote!($($expected)*).to_string()),
                Err(err) => panic!("{}", err),
            }
        }
    };
}

t! {
    name: index,
    source: ("/"),
    expected: {
        fn route() -> tsukuyomi::app::route::Builder<()> {
            tsukuyomi::app::route()
                .uri("/".parse().expect("this is a bug"))
        }
    },
}

t! {
    name: single_param,
    source: ("/:id"),
    expected: {
        fn route<T0>() -> tsukuyomi::app::route::Builder<
            impl tsukuyomi::extractor::Extractor<Output = (T0,), Error = tsukuyomi::error::Error>,
        >
        where
            T0: tsukuyomi::extractor::param::FromParam,
        {
            tsukuyomi::app::route()
                .uri("/:id".parse().expect("this is a bug"))
                .with(tsukuyomi::extractor::param::pos(0usize))
        }
    },
}

t! {
    name: wildcard_param,
    source: ("/*path"),
    expected: {
        fn route<T0>() -> tsukuyomi::app::route::Builder<
            impl tsukuyomi::extractor::Extractor<Output = (T0,), Error = tsukuyomi::error::Error>,
        >
        where
            T0: tsukuyomi::extractor::param::FromParam,
        {
            tsukuyomi::app::route()
                .uri("/*path".parse().expect("this is a bug"))
                .with(tsukuyomi::extractor::param::wildcard())
        }
    },
}

t! {
    name: compound_params,
    source: ("/:id/people/:name/*path"),
    expected: {
        fn route<T0, T1, T2>() -> tsukuyomi::app::route::Builder<
            impl tsukuyomi::extractor::Extractor<Output = (T0, T1, T2,), Error = tsukuyomi::error::Error>,
        >
        where
            T0: tsukuyomi::extractor::param::FromParam,
            T1: tsukuyomi::extractor::param::FromParam,
            T2: tsukuyomi::extractor::param::FromParam,
        {
            tsukuyomi::app::route()
                .uri("/:id/people/:name/*path".parse().expect("this is a bug"))
                .with(tsukuyomi::extractor::param::pos(0usize))
                .with(tsukuyomi::extractor::param::pos(1usize))
                .with(tsukuyomi::extractor::param::wildcard())
        }
    },
}
