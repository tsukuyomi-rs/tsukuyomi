use proc_macro2::TokenStream;
use quote::quote;
use std::str::FromStr;
use syn::parse_quote;
use syn::FnArg;

#[derive(Debug, Copy, Clone)]
pub enum HandlerMode {
    Async,
    Ready,
}

impl FromStr for HandlerMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "async" | "" => Ok(HandlerMode::Async),
            "ready" => Ok(HandlerMode::Ready),
            s => Err(format!("invalid handler mode: `{}'", s)),
        }
    }
}

pub fn handler(item: syn::ItemFn, mode: HandlerMode) -> TokenStream {
    let mut inner = item.clone();
    inner.ident = syn::Ident::new("inner", inner.ident.span());

    if item.decl.inputs.is_empty() {
        inner.decl.inputs.push_value(parse_quote!((): ()));
    } else if item.decl.inputs.len() > 1 {
        inner.decl.inputs = Default::default();
        inner.decl.inputs.push_value({
            let patterns = item.decl.inputs.iter().map(|arg| match arg {
                FnArg::SelfRef(..) => panic!(),
                FnArg::SelfValue(..) => panic!(),
                FnArg::Captured(syn::ArgCaptured { pat, .. }) => pat,
                FnArg::Inferred(..) => panic!(),
                FnArg::Ignored(..) => panic!(),
            });

            let types = item.decl.inputs.iter().map(|arg| match arg {
                FnArg::SelfRef(..) => panic!(),
                FnArg::SelfValue(..) => panic!(),
                FnArg::Captured(syn::ArgCaptured { ty, .. }) => ty,
                FnArg::Inferred(..) => panic!(),
                FnArg::Ignored(..) => panic!(),
            });

            parse_quote!((#(#patterns),*) : (#(#types),*))
        });
    }

    match mode {
        HandlerMode::Async => {
            let vis = &item.vis;
            let ident = &item.ident;
            quote::quote!{
                #vis fn #ident(input: &mut tsukuyomi::input::Input<'_>) -> tsukuyomi::handler::Handle {
                    #inner
                    tsukuyomi::handler::private::handle_async(input, inner)
                }
            }
        }
        HandlerMode::Ready => {
            let vis = &item.vis;
            let ident = &item.ident;
            quote::quote!{
                #vis fn #ident(input: &mut tsukuyomi::input::Input<'_>) -> tsukuyomi::handler::Handle {
                    #inner
                    tsukuyomi::handler::private::handle_ready(input, inner)
                }
            }
        }
    }
}
