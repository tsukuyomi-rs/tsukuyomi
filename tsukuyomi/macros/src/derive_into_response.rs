use {
    proc_macro2::{Span, TokenStream},
    quote::*,
    std::fmt::Display,
    syn::{
        parse::{Error as ParseError, Result as ParseResult},
        spanned::Spanned,
        DeriveInput,
    },
};

fn parse_error<T>(message: T) -> ParseError
where
    T: Display,
{
    ParseError::new(Span::call_site(), message)
}

fn parse_error_at<P, T>(pos: &P, message: T) -> ParseError
where
    T: Display,
    P: Spanned,
{
    ParseError::new(pos.span(), message)
}

fn collect_attrs(attrs: &[syn::Attribute]) -> ParseResult<Option<syn::Path>> {
    let mut meta = None;
    for attr in attrs {
        let m = attr.parse_meta()?;
        if m.name() == "response" {
            meta = Some(m);
        }
    }

    let meta_list = match meta {
        Some(syn::Meta::List(inner)) => inner,
        Some(..) => return Err(parse_error("attribute 'response' has incorrect type")),
        None => return Ok(None),
    };

    let mut into_response = None;
    for nm_item in meta_list.nested {
        if let syn::NestedMeta::Meta(ref item) = nm_item {
            if let syn::Meta::NameValue(ref pair) = item {
                match pair.ident.to_string().as_ref() {
                    "with" => {
                        if let syn::Lit::Str(ref lit) = pair.lit {
                            into_response = lit.parse().map(Some).unwrap();
                        } else {
                            return Err(parse_error_at(&pair.lit, "the literal must be string"));
                        }
                    }
                    _ => return Err(parse_error_at(&pair.ident, "unsupported field")),
                }
            }
        }
    }

    Ok(into_response)
}

pub fn parse(input: DeriveInput) -> ParseResult<ResponderInput> {
    let into_response = collect_attrs(&input.attrs)?;
    Ok(ResponderInput {
        into_response,
        input,
    })
}

#[derive(Debug)]
pub struct ResponderInput {
    into_response: Option<syn::Path>,
    input: DeriveInput,
}

impl ResponderInput {
    fn derive_explicit(&self, into_response: &syn::Path) -> TokenStream {
        quote!(
            #into_response(self, input)
                .map(|response| response.map(Into::into))
                .map_err(Into::into)
        )
    }

    #[allow(nonstandard_style)]
    fn derive_struct(&self, data: &syn::DataStruct) -> ParseResult<TokenStream> {
        let Self_ = &self.input.ident;
        let into_response = quote!(tsukuyomi::output::internal::into_response);
        let unit_respond_to = quote!(#into_response((), input));
        match data.fields {
            syn::Fields::Unit => Ok(unit_respond_to),
            syn::Fields::Unnamed(ref fields) => match fields.unnamed.len() {
                0 => Ok(unit_respond_to),
                1 => Ok(quote!(match self {
                    #Self_(__arg_0) => #into_response(__arg_0, input),
                })),
                _ => Err(parse_error_at(fields, "multiple fields is not supported.")),
            },

            syn::Fields::Named(ref fields) => match fields.named.len() {
                0 => Ok(unit_respond_to),
                1 => {
                    let field = &fields.named[0].ident;
                    Ok(quote!(match self {
                        #Self_ { #field: __arg_0, } => #into_response(__arg_0, input),
                    }))
                }
                _ => Err(parse_error_at(fields, "multiple fields is not supported.")),
            },
        }
    }

    fn derive_enum(&self, data: &syn::DataEnum) -> ParseResult<TokenStream> {
        let variants: Vec<_> = data
            .variants
            .iter()
            .map(|variant| self.derive_enum_variant(variant))
            .collect::<syn::parse::Result<_>>()?;

        Ok(quote!(match self {
            #( #variants, )*
        }))
    }

    #[allow(nonstandard_style)]
    fn derive_enum_variant(&self, variant: &syn::Variant) -> ParseResult<TokenStream> {
        let Self_ = &self.input.ident;
        let Variant = &variant.ident;
        let into_response = quote!(tsukuyomi::output::internal::into_response);
        match variant.fields {
            syn::Fields::Unit => Ok(quote!(#Self_ :: #Variant => #into_response((), input))),

            syn::Fields::Unnamed(ref fields) => match fields.unnamed.len() {
                0 => Ok(quote!(#Self_ :: #Variant () => #into_response((), input))),
                1 => Ok(quote!(#Self_ :: #Variant (__arg_0) => #into_response(__arg_0, input))),
                _ => Err(parse_error_at(fields, "multiple fields is not supported.")),
            },

            syn::Fields::Named(ref fields) => match fields.named.len() {
                0 => Ok(quote!(#Self_ :: #Variant {} => #into_response((), input))),
                1 => {
                    let field = &fields.named[0].ident;
                    Ok(
                        quote!(#Self_ :: #Variant { #field: __arg_0, } => #into_response(__arg_0, input)),
                    )
                }
                _ => Err(parse_error_at(fields, "multiple fields is not supported.")),
            },
        }
    }

    #[allow(nonstandard_style)]
    pub fn derive(&self) -> ParseResult<TokenStream> {
        let derived = match (&self.into_response, &self.input.data) {
            (Some(into_response), _) => self.derive_explicit(into_response),
            (None, syn::Data::Struct(ref data)) => self.derive_struct(data)?,
            (None, syn::Data::Enum(ref data)) => self.derive_enum(data)?,
            (None, syn::Data::Union(..)) => {
                return Err(parse_error("tagged union is not supported."));
            }
        };

        let Self_ = &self.input.ident;
        let IntoResponse = quote!(tsukuyomi::output::IntoResponse);
        let ResponseBody = quote!(tsukuyomi::output::ResponseBody);
        let Error = quote!(tsukuyomi::error::Error);
        let Input = quote!(tsukuyomi::input::Input);
        let Response = quote!(tsukuyomi::output::internal::Response);

        Ok(quote!(
            impl #IntoResponse for #Self_ {
                type Body = #ResponseBody;
                type Error = #Error;

                #[inline]
                fn into_response(self, input: &mut #Input<'_>) -> Result<#Response<Self::Body>, Self::Error> {
                    #derived
                }
            }
        ))
    }
}

pub fn derive_into_response(input: TokenStream) -> ParseResult<TokenStream> {
    syn::parse2(input)
        .and_then(self::parse)
        .and_then(|input| input.derive())
}

macro_rules! t {
    (
        name: $name:ident,
        source: { $($source:tt)* },
        body: {$($body:tt)*},
    ) => {
        #[test]
        #[allow(nonstandard_style)]
        fn $name() {
            use quote::*;

            let source: syn::DeriveInput = syn::parse_quote!($($source)*);
            let output = derive_into_response(quote!(#source)).unwrap();

            let expected = {
                let Self_ = &source.ident;
                quote! {
                    impl tsukuyomi::output::IntoResponse for #Self_ {
                        type Body = tsukuyomi::output::ResponseBody;
                        type Error = tsukuyomi::error::Error;

                        #[inline]
                        fn into_response(self, input: &mut tsukuyomi::input::Input<'_>)
                            -> Result<tsukuyomi::output::internal::Response<Self::Body>, Self::Error>
                        {
                            $($body)*
                        }
                    }
                }
            };

            assert_eq!(output.to_string(), expected.to_string());
        }
    };

    (
        name: $name:ident,
        source: { $($source:tt)* },
        error: $message:expr,
    ) => {
        #[test]
        #[allow(nonstandard_style)]
        fn $name() {
            use quote::*;
            let source: syn::DeriveInput = syn::parse_quote!($($source)*);
            match derive_into_response(quote!(#source)) {
                Ok(..) => panic!("the derivation should be failed"),
                Err(e) => assert_eq!(e.to_string(), $message.to_string()),
            }
        }
    }
}

t! {
    name: test_unit_struct,
    source: { struct A; },
    body: { tsukuyomi::output::internal::into_response((), input) },
}

t! {
    name: test_unnamed_struct,
    source: {
        struct A(String);
    },
    body: {
        match self {
            A(__arg_0) => tsukuyomi::output::internal::into_response(__arg_0, input),
        }
    },
}

t! {
    name: test_unnamed_struct_with_empty_fields,
    source: {
        struct A();
    },
    body: {
        tsukuyomi::output::internal::into_response((), input)
    },
}

t! {
    name: test_named_struct,
    source: {
        struct A {
            b: B,
        }
    },
    body: {
        match self {
            A { b: __arg_0, } => tsukuyomi::output::internal::into_response(__arg_0, input),
        }
    },
}

t! {
    name: test_named_struct_with_empty_fields,
    source: {
        struct A {}
    },
    body: {
        tsukuyomi::output::internal::into_response((), input)
    },
}

t! {
    name: test_enum,
    source: {
        enum Either {
            A(A),
            B { b: B },
            C,
            D(),
            E {},
        }
    },
    body: {
        match self {
            Either::A(__arg_0) => tsukuyomi::output::internal::into_response(__arg_0, input),
            Either::B { b: __arg_0, } => tsukuyomi::output::internal::into_response(__arg_0, input),
            Either::C => tsukuyomi::output::internal::into_response((), input),
            Either::D() => tsukuyomi::output::internal::into_response((), input),
            Either::E {} => tsukuyomi::output::internal::into_response((), input),
        }
    },
}

t! {
    name: test_explicit_struct,
    source: {
        #[response(with = "my::into_response")]
        struct A {
            x: X,
            y: Y,
        }
    },
    body: {
        my::into_response(self, input)
            .map(|response| response.map(Into::into))
            .map_err(Into::into)
    },
}

t! {
    name: test_unsupported_union,
    source: {
        union A {}
    },
    error: "tagged union is not supported.",
}

t! {
    name: test_unnamed_struct_with_multiple_fields,
    source: {
        struct A(B, C);
    },
    error: "multiple fields is not supported.",
}

t! {
    name: test_named_struct_with_multiple_fields,
    source: {
        struct A {
            b: B,
            c: C,
        }
    },
    error: "multiple fields is not supported.",
}

t! {
    name: test_enum_contains_unnamed_multiple_fields,
    source: {
        enum A {
            B(C, D),
        }
    },
    error: "multiple fields is not supported.",
}

t! {
    name: test_enum_contains_named_multiple_fields,
    source: {
        enum A {
            B {
                c: C,
                d: D,
            },
        }
    },
    error: "multiple fields is not supported.",
}
