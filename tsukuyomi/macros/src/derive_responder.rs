use proc_macro2::{Span, TokenStream};
use quote::*;
use syn::spanned::Spanned;
use syn::DeriveInput;

fn parse_error<T>(message: T) -> syn::parse::Error
where
    T: std::fmt::Display,
{
    syn::parse::Error::new(Span::call_site(), message)
}

fn parse_error_at<P, T>(pos: &P, message: T) -> syn::parse::Error
where
    T: std::fmt::Display,
    P: Spanned,
{
    syn::parse::Error::new(pos.span(), message)
}

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
        Some(..) => return Err(parse_error("attribute 'responder' has incorrect type")),
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
                            return Err(parse_error_at(&pair.lit, "the literal must be string"));
                        }
                    }
                    _ => return Err(parse_error_at(&pair.ident, "unsupported field")),
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

impl ResponderInput {
    fn derive_explicit(&self, respond_to: &syn::Path) -> TokenStream {
        quote!(
            #respond_to(self, input)
                .map(|response| response.map(Into::into))
                .map_err(Into::into)
        )
    }

    fn derive_struct(&self, _data: &syn::DataStruct) -> syn::parse::Result<TokenStream> {
        Err(parse_error("explicit responder function is required."))
    }

    #[allow(nonstandard_style)]
    fn derive_enum(&self, data: &syn::DataEnum) -> syn::parse::Result<TokenStream> {
        let variants: Vec<_> = data
            .variants
            .iter()
            .map(|variant| {
                let Self_ = &self.input.ident;
                let ident = &variant.ident;
                let respond_to = quote!(tsukuyomi::output::internal::respond_to);
                match variant.fields {
                    syn::Fields::Unit => Ok(quote!(#Self_::#ident => #respond_to((), input))),

                    syn::Fields::Unnamed(ref fields) => {
                        if fields.unnamed.len() == 1 {
                            Ok(quote!(#Self_::#ident (__arg_0) => #respond_to(__arg_0, input)))
                        } else {
                            Err(parse_error_at(fields, "multiple fields is not supported."))
                        }
                    }

                    syn::Fields::Named(ref fields) => {
                        if fields.named.len() == 1 {
                            let field = &fields.named[0].ident;
                            Ok(quote!(#Self_::#ident { #field: __arg_0, } => #respond_to(__arg_0, input)))
                        } else {
                            Err(parse_error_at(fields, "multiple fields is not supported."))
                        }
                    },
                }
            }).collect::<syn::parse::Result<_>>()?;

        Ok(quote!(match self {
            #( #variants, )*
        }))
    }

    #[allow(nonstandard_style)]
    pub fn derive(&self) -> syn::parse::Result<TokenStream> {
        let derived = match (&self.respond_to, &self.input.data) {
            (Some(respond_to), _) => self.derive_explicit(respond_to),
            (None, syn::Data::Struct(ref data)) => self.derive_struct(data)?,
            (None, syn::Data::Enum(ref data)) => self.derive_enum(data)?,
            (None, syn::Data::Union(..)) => {
                return Err(parse_error("tagged union is not supported."))
            }
        };

        let Self_ = &self.input.ident;
        let Responder = quote!(tsukuyomi::output::Responder);
        let ResponseBody = quote!(tsukuyomi::output::ResponseBody);
        let Error = quote!(tsukuyomi::error::Error);
        let Input = quote!(tsukuyomi::input::Input);
        let Response = quote!(tsukuyomi::output::internal::Response);

        Ok(quote!(
            impl #Responder for #Self_ {
                type Body = #ResponseBody;
                type Error = #Error;

                #[inline]
                fn respond_to(self, input: &mut #Input<'_>) -> Result<#Response<Self::Body>, Self::Error> {
                    #derived
                }
            }
        ))
    }
}
