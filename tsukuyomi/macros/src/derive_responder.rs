#![allow(clippy::large_enum_variant)]

use {
    proc_macro2::TokenStream, //
    quote::*,
};

pub fn derive(input: TokenStream) -> syn::parse::Result<TokenStream> {
    let input: Input = syn::parse2(input)?;
    let ctx = Context {
        ident: &input.ident,
        generics: &input.generics,
        bounds: &input.bounds,
        preset_path: &input.preset_path,
    };

    Ok(ctx.to_tokens())
}

#[derive(Debug)]
struct Input {
    ident: syn::Ident,
    generics: syn::Generics,
    bounds: Option<Vec<syn::WherePredicate>>,
    preset_path: syn::Path,
}

mod parsing {
    use {
        super::Input,
        proc_macro2::Span,
        std::fmt::Display,
        syn::{
            parse, //
            spanned::Spanned,
        },
    };

    fn parse_error<T>(message: T) -> parse::Error
    where
        T: Display,
    {
        parse::Error::new(Span::call_site(), message)
    }

    fn parse_error_at<P, T>(pos: &P, message: T) -> parse::Error
    where
        T: Display,
        P: Spanned,
    {
        parse::Error::new(pos.span(), message)
    }

    fn parse_literal<T: parse::Parse>(lit: &syn::Lit) -> parse::Result<T> {
        match lit {
            syn::Lit::Str(ref lit) => lit.parse(),
            _ => Err(parse_error_at(lit, "the literal must be string")),
        }
    }

    impl parse::Parse for Input {
        fn parse(input: parse::ParseStream<'_>) -> parse::Result<Self> {
            let input: syn::DeriveInput = input.parse()?;

            let mut preset_path: Option<syn::Path> = None;
            let mut bounds: Option<Vec<syn::WherePredicate>> = None;

            for attr in &input.attrs {
                let m = attr.parse_meta()?;
                if m.name() != "response" {
                    continue;
                }

                let meta_list = match m {
                    syn::Meta::List(inner) => inner,
                    m => {
                        return Err(parse_error_at(
                            &m,
                            "the attribute 'response' has incorrect type",
                        ));
                    }
                };

                for nm_item in meta_list.nested {
                    if let syn::NestedMeta::Meta(syn::Meta::NameValue(ref pair)) = nm_item {
                        match pair.ident.to_string().as_ref() {
                            "preset" => {
                                if preset_path.is_some() {
                                    return Err(parse_error_at(
                                        &pair,
                                        "the parameter 'preset' has already been provided",
                                    ));
                                }
                                let path = parse_literal(&pair.lit)?;
                                preset_path = Some(path);
                            }
                            "bound" => {
                                let bound = parse_literal(&pair.lit)?;
                                bounds.get_or_insert_with(Default::default).push(bound);
                            }
                            s => {
                                return Err(parse_error_at(
                                    &pair.ident,
                                    format!("unsupported field: '{}'", s),
                                ));
                            }
                        }
                    }
                }
            }

            let preset_path =
                preset_path.ok_or_else(|| parse_error("missing parameter `preset`"))?;

            Ok(Self {
                ident: input.ident,
                generics: input.generics,
                bounds,
                preset_path,
            })
        }
    }
}

#[derive(Debug)]
struct Context<'a> {
    ident: &'a syn::Ident,
    generics: &'a syn::Generics,
    bounds: &'a Option<Vec<syn::WherePredicate>>,
    preset_path: &'a syn::Path,
}

impl<'a> Context<'a> {
    #[allow(nonstandard_style)]
    pub fn to_tokens(&self) -> TokenStream {
        // The path of items used in the derived impl.
        let Self_ = self.ident;
        let Responder: syn::Path = syn::parse_quote!(tsukuyomi::output::Responder);
        let Preset: syn::Path = syn::parse_quote!(tsukuyomi::output::preset::Preset);

        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        // appends additional bounds specified by the macro user to where clause.
        let mut where_clause = where_clause.cloned();
        if let Some(ref bounds) = self.bounds {
            where_clause
                .get_or_insert_with(|| syn::WhereClause {
                    where_token: Default::default(),
                    predicates: Default::default(),
                })
                .predicates
                .extend(bounds.iter().cloned());
        }

        // The path of types drawn at the position of the associated type.
        let preset_path = &self.preset_path;
        where_clause
            .get_or_insert_with(|| syn::WhereClause {
                where_token: Default::default(),
                predicates: Default::default(),
            })
            .predicates
            .push(syn::parse_quote!(#preset_path: #Preset<Self>));

        // appends the trailing comma if not exist.
        if let Some(where_clause) = &mut where_clause {
            if !where_clause.predicates.empty_or_trailing() {
                where_clause.predicates.push_punct(Default::default());
            }
        }

        quote!(
            impl #impl_generics #Responder for #Self_ #ty_generics
            #where_clause
            {
                type Upgrade = < #preset_path as #Preset<Self> >::Upgrade;
                type Error = < #preset_path as #Preset<Self> >::Error;
                type Respond = < #preset_path as #Preset<Self> >::Respond;

                #[inline]
                fn respond(self) -> Self::Respond {
                    < #preset_path as #Preset<Self> >::respond(self)
                }
            }
        )
    }
}

// ==== test ====

#[cfg(test)]
mod tests {
    macro_rules! t {
        (
            name: $name:ident,
            source: { $($source:tt)* },
            expected: {$($expected:tt)*},
        ) => {
            #[test]
            fn $name() {
                use quote::*;
                let output = super::derive(quote!($($source)*)).unwrap();
                let expected = quote!($($expected)*);
                assert_eq!(output.to_string(), expected.to_string());
            }
        };

        (
            name: $name:ident,
            source: { $($source:tt)* },
            error: $message:expr,
        ) => {
            #[test]
            fn $name() {
                use quote::*;
                match super::derive(quote!($($source)*)) {
                    Ok(..) => panic!("the derivation should be failed"),
                    Err(e) => assert_eq!(e.to_string(), $message.to_string()),
                }
            }
        }
    }

    t! {
        name: explicit_preset,
        source: {
            #[response(preset = "my::Preset")]
            struct A {
                x: X,
                y: Y,
            }
        },
        expected: {
            impl tsukuyomi::output::Responder for A
            where
                my::Preset: tsukuyomi::output::preset::Preset<Self>,
            {
                type Upgrade = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Upgrade;
                type Error = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Error;
                type Respond = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Respond;

                #[inline]
                fn respond(self) -> Self::Respond {
                    <my::Preset as tsukuyomi::output::preset::Preset<Self> >::respond(self)
                }
            }
        },
    }

    t! {
        name: explicit_preset_additional_bounds,
        source: {
            #[response(
                preset = "my::Preset",
                bound = "X: Foo",
                bound = "Y: Foo",
            )]
            struct A<X, Y> {
                x: X,
                y: Y,
            }
        },
        expected: {
            impl<X, Y> tsukuyomi::output::Responder for A<X, Y>
            where
                X: Foo,
                Y: Foo,
                my::Preset: tsukuyomi::output::preset::Preset<Self>,
            {
                type Upgrade = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Upgrade;
                type Error = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Error;
                type Respond = < my::Preset as tsukuyomi::output::preset::Preset<Self> >::Respond;

                #[inline]
                fn respond(self) -> Self::Respond {
                    <my::Preset as tsukuyomi::output::preset::Preset<Self> >::respond(self)
                }
            }
        },
    }

    t! {
        name: failcase_missing_preset,
        source: {
            struct A(B, C);
        },
        error: "missing parameter `preset`",
    }
}
