use failure::Error;
use proc_macro2::{Span, TokenStream};
use quote::*;
use syn;

pub fn async_handler(_attr: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let item: syn::ItemFn = syn::parse2(item)?;

    let context = Context { item: item };
    context.validate()?;

    Ok(context.generate().into_token_stream())
}

// ====

struct Context {
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

        Ok(())
    }

    fn generate(&self) -> syn::ItemFn {
        let inner = self.generate_inner_item();
        self.generate_new_item(inner)
    }

    fn generate_inner_item(&self) -> syn::ItemFn {
        let mut inner = self.item.clone();
        inner.ident = syn::Ident::new("inner", Span::call_site());
        inner
    }

    fn generate_new_item(&self, inner: syn::ItemFn) -> syn::ItemFn {
        let vis = &self.item.vis;
        let ident = &self.item.ident;

        let input: syn::Ident = if self.num_inputs() == 0 {
            syn::Ident::new("_input", Span::call_site())
        } else {
            syn::Ident::new("input", Span::call_site())
        };

        let call: syn::Expr = match self.num_inputs() {
            0 => parse_quote!(inner()),
            1 => parse_quote!(inner(input)),
            _ => unreachable!(),
        };

        let body: syn::Expr = parse_quote!({
            ::tsukuyomi::handler::Handle::wrap_async(#call)
        });

        parse_quote!{
            #vis fn #ident(#input: &mut ::tsukuyomi::input::Input) -> ::tsukuyomi::handler::Handle {
                #inner
                #body
            }
        }
    }
}
