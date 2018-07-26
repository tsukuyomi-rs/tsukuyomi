use failure::Error;
use proc_macro2::{Span, TokenStream};
use quote::*;
use syn;

pub fn handler(attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let item: syn::ItemFn = syn::parse2(item)?;
    let mode = detect_mode(&attr, &item)?;

    let context = Context { item: item, mode: mode };
    context.validate()?;

    Ok(context.generate().into_token_stream())
}

// ====

#[derive(Debug, Copy, Clone, PartialEq)]
enum HandlerMode {
    Ready,
    Async,
}

fn detect_mode(attr: &TokenStream, _item: &syn::ItemFn) -> Result<HandlerMode, Error> {
    // FIXME: detect the keyword `async`
    match &*attr.to_string() {
        "" => Ok(HandlerMode::Ready),
        "async" => Ok(HandlerMode::Async),
        s => bail!("The mode `{}` is invalid.", s),
    }
}

struct Context {
    mode: HandlerMode,
    item: syn::ItemFn,
}

impl Context {
    fn num_inputs(&self) -> usize {
        self.item.decl.inputs.len()
    }

    fn validate(&self) -> Result<(), Error> {
        if self.item.unsafety.is_some() {
            bail!("unsafe fn is not supported.");
        }

        if self.item.abi.is_some() {
            bail!("The handler function cannot be used via FFI bound.");
        }

        if self.num_inputs() > 1 {
            bail!("Too many arguments");
        }

        if self.mode == HandlerMode::Async && self.num_inputs() != 0 {
            bail!("The number of arguments in #[async] handler must be zero.");
        }

        Ok(())
    }

    fn generate(&self) -> syn::ItemFn {
        let inner = self.generate_inner_item();
        self.generate_new_item(inner)
    }

    fn generate_inner_item(&self) -> syn::ItemFn {
        let mut inner = self.item.clone();

        inner.ident = syn::Ident::new("inner", Span::call_site());

        if self.mode == HandlerMode::Async {
            inner.attrs.push(parse_quote!(#[async]));
        }

        inner
    }

    fn generate_new_item(&self, inner: syn::ItemFn) -> syn::ItemFn {
        let vis = &self.item.vis;
        let ident = &self.item.ident;

        let input: syn::Ident = match self.mode {
            HandlerMode::Ready if self.num_inputs() == 0 => syn::Ident::new("_input", Span::call_site()),
            _ => syn::Ident::new("input", Span::call_site()),
        };

        let prelude: Option<syn::Stmt> = match self.mode {
            HandlerMode::Async => Some(parse_quote!(use futures::prelude::async;)),
            _ => None,
        };

        let call: syn::Expr = match self.num_inputs() {
            0 => parse_quote!(inner()),
            1 => parse_quote!(inner(input)),
            _ => unreachable!(),
        };

        let body: syn::Expr = match self.mode {
            HandlerMode::Ready => parse_quote!({
                ::tsukuyomi::handler::Handle::ready(
                    ::tsukuyomi::output::Responder::respond_to(#call, input)
                )
            }),
            HandlerMode::Async => parse_quote!({
                ::tsukuyomi::handler::Handle::wrap_async(#call)
            }),
        };

        parse_quote!{
            #vis fn #ident(#input: &mut ::tsukuyomi::input::Input) -> ::tsukuyomi::handler::Handle {
                #prelude
                #inner
                #body
            }
        }
    }
}
