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
        kind: &input.kind,
    };

    Ok(ctx.to_tokens())
}

#[derive(Debug)]
struct Input {
    ident: syn::Ident,
    generics: syn::Generics,
    bounds: Option<Vec<syn::WherePredicate>>,
    kind: InputKind,
}

#[derive(Debug)]
enum InputKind {
    Struct(Target),
    Enum(Vec<Variant>),
    UsePreset(syn::Path),
}

#[derive(Debug)]
enum Target {
    NamedField(Option<syn::Field>),
    UnnamedField(Option<syn::Field>),
    Unit,
}

#[derive(Debug)]
struct Variant {
    ident: syn::Ident,
    target: Target,
}

mod parsing {
    use {
        super::{Input, InputKind, Target, Variant},
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

            // The kind when specifying the path to `into_response` explicitly.
            enum ExplicitKind {
                // New style - a type that implements Preset<T>
                Preset(syn::Path),
            }

            let mut explicit_path: Option<ExplicitKind> = None;
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
                                if explicit_path.is_some() {
                                    return Err(parse_error_at(
                                        &pair,
                                        "the parameter 'preset' has already been provided",
                                    ));
                                }
                                let path = parse_literal(&pair.lit)?;
                                explicit_path = Some(ExplicitKind::Preset(path));
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

            let kind = match explicit_path {
                Some(ExplicitKind::Preset(path)) => InputKind::UsePreset(path),
                None => match input.data {
                    syn::Data::Struct(data) => {
                        let field = match data.fields {
                            syn::Fields::Unit => Target::Unit,
                            syn::Fields::Unnamed(fields) => {
                                if fields.unnamed.len() > 1 {
                                    return Err(parse_error_at(
                                        &fields,
                                        "multiple fields is not supported.",
                                    ));
                                }
                                let field = fields.unnamed.into_iter().next();
                                Target::UnnamedField(field)
                            }
                            syn::Fields::Named(fields) => {
                                if fields.named.len() > 1 {
                                    return Err(parse_error_at(
                                        &fields,
                                        "multiple fields is not supported.",
                                    ));
                                }
                                let field = fields.named.into_iter().next();
                                Target::NamedField(field)
                            }
                        };
                        InputKind::Struct(field)
                    }
                    syn::Data::Enum(data) => {
                        let mut variants = vec![];
                        for variant in data.variants {
                            match variant.fields {
                                syn::Fields::Unit => variants.push(Variant {
                                    ident: variant.ident,
                                    target: Target::Unit,
                                }),
                                syn::Fields::Unnamed(fields) => {
                                    if fields.unnamed.len() > 1 {
                                        return Err(parse_error_at(
                                            &fields,
                                            "multiple fields is not supported.",
                                        ));
                                    }
                                    let field = fields.unnamed.into_iter().next();
                                    variants.push(Variant {
                                        ident: variant.ident,
                                        target: Target::UnnamedField(field),
                                    });
                                }

                                syn::Fields::Named(fields) => {
                                    if fields.named.len() > 1 {
                                        return Err(parse_error_at(
                                            &fields,
                                            "multiple fields is not supported.",
                                        ));
                                    }
                                    let field = fields.named.into_iter().next();
                                    variants.push(Variant {
                                        ident: variant.ident,
                                        target: Target::NamedField(field),
                                    });
                                }
                            }
                        }
                        InputKind::Enum(variants)
                    }
                    syn::Data::Union(..) => {
                        return Err(parse_error("tagged union is not supported."));
                    }
                },
            };

            Ok(Self {
                ident: input.ident,
                generics: input.generics,
                bounds,
                kind,
            })
        }
    }
}

#[derive(Debug)]
struct Context<'a> {
    ident: &'a syn::Ident,
    generics: &'a syn::Generics,
    bounds: &'a Option<Vec<syn::WherePredicate>>,
    kind: &'a InputKind,
}

impl<'a> Context<'a> {
    #[allow(nonstandard_style)]
    pub fn to_tokens(&self) -> TokenStream {
        // The path of items used in the derived impl.
        let Self_ = self.ident;
        let IntoResponse: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::IntoResponse);
        let Result: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Result);
        let Request: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Request);
        let Preset: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Preset);

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
        let body: TokenStream;
        match &self.kind {
            InputKind::UsePreset(path) => {
                where_clause
                    .get_or_insert_with(|| syn::WhereClause {
                        where_token: Default::default(),
                        predicates: Default::default(),
                    })
                    .predicates
                    .push(syn::parse_quote!(#path: #Preset<Self>));

                body = quote!(< #path as #Preset<Self> >::into_response(self, request));
            }

            InputKind::Struct(target) => match target {
                Target::Unit | Target::UnnamedField(None) | Target::NamedField(None) => {
                    body = quote!(#IntoResponse::into_response((), request));
                }

                Target::UnnamedField(Some(field)) => {
                    let bounded_ty = &field.ty;
                    where_clause
                        .get_or_insert_with(|| syn::WhereClause {
                            where_token: Default::default(),
                            predicates: Default::default(),
                        })
                        .predicates
                        .push(syn::parse_quote!(#bounded_ty: #IntoResponse));
                    body = quote!(match self {
                        #Self_(__arg_0) => #IntoResponse::into_response(__arg_0, request),
                    });
                }

                Target::NamedField(Some(field)) => {
                    let bounded_ty = &field.ty;
                    let field_ident = &field.ident;
                    where_clause
                        .get_or_insert_with(|| syn::WhereClause {
                            where_token: Default::default(),
                            predicates: Default::default(),
                        })
                        .predicates
                        .push(syn::parse_quote!(#bounded_ty: #IntoResponse));
                    body = quote!(match self {
                        #Self_ { #field_ident: __arg_0, } => #IntoResponse::into_response(__arg_0, request),
                    });
                }
            },

            InputKind::Enum(variants) => {
                let variants = variants.iter().map(|variant| {
                    let Variant = &variant.ident;
                    match &variant.target {
                        Target::Unit => quote!(
                            #Self_ :: #Variant => #IntoResponse::into_response((), request)
                        ),

                        Target::UnnamedField(None) => {
                            quote!(#Self_ :: #Variant () => #IntoResponse::into_response((), request))
                        }
                        Target::UnnamedField(Some(field)) => {
                            let bounded_ty = &field.ty;
                            where_clause
                                .get_or_insert_with(|| syn::WhereClause {
                                    where_token: Default::default(),
                                    predicates: Default::default(),
                                })
                                .predicates
                                .push(syn::parse_quote!(#bounded_ty: #IntoResponse));
                            quote!(#Self_ :: #Variant (__arg_0) => #IntoResponse::into_response(__arg_0, request))
                        }

                        Target::NamedField(None) => {
                            quote!(#Self_ :: #Variant {} => #IntoResponse::into_response((), request))
                        }
                        Target::NamedField(Some(field)) => {
                            let bounded_ty = &field.ty;
                            where_clause
                                .get_or_insert_with(|| syn::WhereClause {
                                    where_token: Default::default(),
                                    predicates: Default::default(),
                                })
                                .predicates
                                .push(syn::parse_quote!(#bounded_ty: #IntoResponse));
                            let field = &field.ident;
                            quote!(#Self_ :: #Variant { #field: __arg_0, } => #IntoResponse::into_response(__arg_0, request))
                        }
                    }
                });

                body = quote!(match self {
                    #( #variants, )*
                });
            }
        };

        // appends the trailing comma if not exist.
        if let Some(where_clause) = &mut where_clause {
            if !where_clause.predicates.empty_or_trailing() {
                where_clause.predicates.push_punct(Default::default());
            }
        }

        quote!(
            impl #impl_generics #IntoResponse for #Self_ #ty_generics
            #where_clause
            {
                #[inline]
                fn into_response(self, request: &#Request<()>) -> #Result {
                    #body
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
        name: implicit_unit_struct,
        source: { struct A; },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                }
            }
        },
    }

    t! {
        name: implicit_unnamed_struct,
        source: {
            struct A(String);
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A
            where
                String: tsukuyomi::output::internal::IntoResponse,
            {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    match self {
                        A(__arg_0) =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request),
                    }
                }
            }
        },
    }

    t! {
        name: implicit_unnamed_struct_with_empty_fields,
        source: {
            struct A();
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                }
            }
        },
    }

    t! {
        name: implicit_named_struct,
        source: {
            struct A {
                b: B,
            }
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A
            where
                B: tsukuyomi::output::internal::IntoResponse,
            {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    match self {
                        A { b: __arg_0, } =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request),
                    }
                }
            }
        },
    }

    t! {
        name: implicit_named_struct_with_empty_fields,
        source: {
            struct A {}
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                }
            }
        },
    }

    t! {
        name: implicit_enum,
        source: {
            enum Either {
                A(A),
                B { b: B },
                C,
                D(),
                E {},
            }
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for Either
            where
                A: tsukuyomi::output::internal::IntoResponse,
                B: tsukuyomi::output::internal::IntoResponse,
            {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    match self {
                        Either::A(__arg_0) =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request),
                        Either::B { b: __arg_0, } =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request),
                        Either::C =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request),
                        Either::D() =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request),
                        Either::E {} =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request),
                    }
                }
            }
        },
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
            impl tsukuyomi::output::internal::IntoResponse for A
            where
                my::Preset: tsukuyomi::output::internal::Preset<Self>,
            {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    <my::Preset as tsukuyomi::output::internal::Preset<Self> >::into_response(self, request)
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
            impl<X, Y> tsukuyomi::output::internal::IntoResponse for A<X, Y>
            where
                X: Foo,
                Y: Foo,
                my::Preset: tsukuyomi::output::internal::Preset<Self>,
            {
                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> tsukuyomi::output::internal::Result {
                    <my::Preset as tsukuyomi::output::internal::Preset<Self> >::into_response(self, request)
                }
            }
        },
    }

    t! {
        name: failcase_unsupported_union,
        source: {
            union A {}
        },
        error: "tagged union is not supported.",
    }

    t! {
        name: failcase_unnamed_struct_with_multiple_fields,
        source: {
            struct A(B, C);
        },
        error: "multiple fields is not supported.",
    }

    t! {
        name: failcase_named_struct_with_multiple_fields,
        source: {
            struct A {
                b: B,
                c: C,
            }
        },
        error: "multiple fields is not supported.",
    }

    t! {
        name: failcase_enum_contains_unnamed_multiple_fields,
        source: {
            enum A {
                B(C, D),
            }
        },
        error: "multiple fields is not supported.",
    }

    t! {
        name: failcase_enum_contains_named_multiple_fields,
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
}
