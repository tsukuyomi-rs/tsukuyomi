use quote::quote;
use syn::parse_quote;
use syn::FnArg;

#[derive(Debug, Copy, Clone)]
pub enum HandleMode {
    Ready,
    Polling,
}

pub fn derive_handler(item: syn::ItemFn, mode: HandleMode) -> syn::ItemFn {
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
        HandleMode::Ready => {
            let vis = &item.vis;
            let ident = &item.ident;
            parse_quote!{
                #vis fn #ident(input: &mut tsukuyomi::input::Input<'_>) -> tsukuyomi::handler::Handle {
                    #inner
                    tsukuyomi::handler::private::handle_ready(input, inner)
                }
            }
        }

        HandleMode::Polling => {
            let vis = &item.vis;
            let ident = &item.ident;
            parse_quote!{
                #vis fn #ident(input: &mut tsukuyomi::input::Input<'_>) -> tsukuyomi::handler::Handle {
                    #inner
                    tsukuyomi::handler::private::handle_async(input, inner)
                }
            }
        }
    }
}
