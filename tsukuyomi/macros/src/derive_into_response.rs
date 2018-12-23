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
    ExplicitWithFnPath(syn::Path),
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

    fn parse_with(lit: &syn::Lit) -> parse::Result<syn::Path> {
        match lit {
            syn::Lit::Str(ref lit) => lit.parse(),
            _ => Err(parse_error_at(lit, "the literal must be string")),
        }
    }

    fn parse_bound(lit: &syn::Lit) -> parse::Result<syn::WherePredicate> {
        match lit {
            syn::Lit::Str(ref lit) => lit.parse(),
            _ => Err(parse_error_at(lit, "the literal must be string")),
        }
    }

    impl parse::Parse for Input {
        fn parse(input: parse::ParseStream<'_>) -> parse::Result<Self> {
            let input: syn::DeriveInput = input.parse()?;

            let mut with = None;
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
                        ))
                    }
                };

                for nm_item in meta_list.nested {
                    if let syn::NestedMeta::Meta(syn::Meta::NameValue(ref pair)) = nm_item {
                        match pair.ident.to_string().as_ref() {
                            "with" => with = parse_with(&pair.lit).map(Some)?,
                            "bound" => {
                                let bound = parse_bound(&pair.lit)?;
                                bounds.get_or_insert_with(Default::default).push(bound);
                            }
                            s => {
                                return Err(parse_error_at(
                                    &pair.ident,
                                    format!("unsupported field: '{}'", s),
                                ))
                            }
                        }
                    }
                }
            }

            let kind = match with {
                Some(with) => InputKind::ExplicitWithFnPath(with),
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
                        return Err(parse_error("tagged union is not supported."))
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
        let Request: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Request);
        let Response: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Response);

        // The path of types drawn at the position of the associated type.
        let ResponseBody: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::ResponseBody);
        let Error: syn::Path = syn::parse_quote!(tsukuyomi::output::internal::Error);

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
                .extend(bounds.into_iter().cloned());
        }

        let body = match &self.kind {
            InputKind::ExplicitWithFnPath(into_response) => quote!(
                #into_response(self, request)
                    .map(|response| response.map(Into::into))
                    .map_err(Into::into)
            ),

            InputKind::Struct(target) => match target {
                Target::Unit | Target::UnnamedField(None) | Target::NamedField(None) => quote!(
                    #IntoResponse::into_response((), request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
                ),

                Target::UnnamedField(Some(field)) => {
                    append_into_response_bound(&mut where_clause, field, &IntoResponse);
                    quote!(match self {
                        #Self_(__arg_0) => #IntoResponse::into_response(__arg_0, request)
                            .map(|response| response.map(Into::into))
                            .map_err(Into::into),
                    })
                }

                Target::NamedField(Some(field)) => {
                    append_into_response_bound(&mut where_clause, field, &IntoResponse);
                    let field = &field.ident;
                    quote!(match self {
                        #Self_ { #field: __arg_0, } => #IntoResponse::into_response(__arg_0, request)
                            .map(|response| response.map(Into::into))
                            .map_err(Into::into),
                    })
                }
            },

            InputKind::Enum(variants) => {
                let variants = variants.into_iter().map(|variant| {
                    let Variant = &variant.ident;
                    match &variant.target {
                        Target::Unit => quote!(
                            #Self_ :: #Variant => #IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into)
                        ),

                        Target::UnnamedField(None) => {
                            quote!(#Self_ :: #Variant () => #IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into))
                        }
                        Target::UnnamedField(Some(field)) => {
                            append_into_response_bound(&mut  where_clause, field, &IntoResponse);
                            quote!(#Self_ :: #Variant (__arg_0) => #IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into))
                        }

                        Target::NamedField(None) => {
                            quote!(#Self_ :: #Variant {} => #IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into))
                        }
                        Target::NamedField(Some(field)) => {
                            append_into_response_bound(&mut  where_clause, field, &IntoResponse);
                            let field = &field.ident;
                            quote!(#Self_ :: #Variant { #field: __arg_0, } => #IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into))
                        }
                    }
                });

                quote!(match self {
                    #( #variants, )*
                })
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
                type Body = #ResponseBody;
                type Error = #Error;

                #[inline]
                fn into_response(self, request: &#Request<()>) -> Result<#Response<Self::Body>, Self::Error> {
                    #body
                }
            }
        )
    }
}

#[allow(nonstandard_style)]
fn append_into_response_bound(
    where_clause: &mut Option<syn::WhereClause>,
    field: &syn::Field,
    IntoResponse: &syn::Path,
) {
    where_clause
        .get_or_insert_with(|| syn::WhereClause {
            where_token: Default::default(),
            predicates: Default::default(),
        })
        .predicates
        .push(syn::WherePredicate::Type(syn::PredicateType {
            lifetimes: None,
            bounded_ty: field.ty.clone(),
            colon_token: Default::default(),
            bounds: vec![syn::TypeParamBound::Trait(syn::TraitBound {
                paren_token: None,
                modifier: syn::TraitBoundModifier::None,
                lifetimes: None,
                path: IntoResponse.clone(),
            })]
            .into_iter()
            .collect(),
        }));
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    match self {
                        A(__arg_0) =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    match self {
                        A { b: __arg_0, } =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    tsukuyomi::output::internal::IntoResponse::into_response((), request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
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
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    match self {
                        Either::A(__arg_0) =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
                        Either::B { b: __arg_0, } =>
                            tsukuyomi::output::internal::IntoResponse::into_response(__arg_0, request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
                        Either::C =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
                        Either::D() =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
                        Either::E {} =>
                            tsukuyomi::output::internal::IntoResponse::into_response((), request)
                                .map(|response| response.map(Into::into))
                                .map_err(Into::into),
                    }
                }
            }
        },
    }

    t! {
        name: explicit_struct,
        source: {
            #[response(with = "my::into_response")]
            struct A {
                x: X,
                y: Y,
            }
        },
        expected: {
            impl tsukuyomi::output::internal::IntoResponse for A {
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    my::into_response(self, request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
                }
            }
        },
    }

    t! {
        name: explicit_with_additional_bounds,
        source: {
            #[response(
                with = "my::into_response",
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
            {
                type Body = tsukuyomi::output::internal::ResponseBody;
                type Error = tsukuyomi::output::internal::Error;

                #[inline]
                fn into_response(
                    self,
                    request: &tsukuyomi::output::internal::Request<()>
                ) -> Result<
                    tsukuyomi::output::internal::Response<Self::Body>,
                    Self::Error
                > {
                    my::into_response(self, request)
                        .map(|response| response.map(Into::into))
                        .map_err(Into::into)
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
