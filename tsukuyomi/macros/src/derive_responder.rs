use proc_macro2::{Span, TokenStream};
use quote::*;
use syn::spanned::Spanned;
use syn::DeriveInput;

fn collect_attrs(attrs: &[syn::Attribute]) -> syn::parse::Result<Option<syn::Path>> {
    let mut meta = None;
    for attr in attrs {
        let m = attr.parse_meta()?;
        if m.name() == "responder" {
            meta = Some(m);
        }
    }

    let meta_list = match meta {
        Some(syn::Meta::List(inner)) => inner,
        Some(..) => {
            return Err(syn::parse::Error::new(
                Span::call_site(),
                "attribute 'responder' has incorrect type",
            ))
        }
        None => return Ok(None),
    };

    let mut respond_to = None;
    for nm_item in meta_list.nested {
        if let syn::NestedMeta::Meta(ref item) = nm_item {
            if let syn::Meta::NameValue(ref pair) = item {
                match pair.ident.to_string().as_ref() {
                    "respond_to" => {
                        if let syn::Lit::Str(ref lit) = pair.lit {
                            respond_to = lit.parse().map(Some).unwrap();
                        } else {
                            return Err(syn::parse::Error::new(
                                pair.lit.span(),
                                "the literal must be string",
                            ));
                        }
                    }
                    _ => {
                        return Err(syn::parse::Error::new(
                            pair.ident.span(),
                            "unsupported field",
                        ))
                    }
                }
            }
        }
    }

    Ok(respond_to)
}

pub fn parse(input: DeriveInput) -> syn::parse::Result<ResponderInput> {
    let respond_to = collect_attrs(&input.attrs)?;

    Ok(ResponderInput { respond_to, input })
}

#[derive(Debug)]
pub struct ResponderInput {
    respond_to: Option<syn::Path>,
    input: DeriveInput,
}

#[allow(nonstandard_style)]
impl ToTokens for ResponderInput {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let respond_to = self
            .respond_to
            .clone()
            .unwrap_or_else(|| syn::parse_quote!(respond_to));

        let Self_ = &self.input.ident;
        let Responder = quote!(tsukuyomi::output::Responder);
        let ResponseBody = quote!(tsukuyomi::output::ResponseBody);
        let Error = quote!(tsukuyomi::error::Error);
        let Input = quote!(tsukuyomi::input::Input);
        let Response = quote!(tsukuyomi::output::internal::Response);

        tokens.append_all(quote!(
            impl #Responder for #Self_ {
                type Body = #ResponseBody;
                type Error = #Error;

                #[inline]
                fn respond_to(self, input: &mut #Input<'_>) -> Result<#Response<Self::Body>, Self::Error> {
                    #respond_to(self, input)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
                }
            }
        ));
    }
}
