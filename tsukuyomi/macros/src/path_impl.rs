use {
    proc_macro2::{Span, TokenStream},
    quote::{quote, ToTokens, TokenStreamExt},
    std::collections::HashSet,
    syn::parse,
};

#[derive(Debug)]
pub struct PathImplInput {
    module: syn::Path,
    comma: syn::Token![,],
    path: syn::LitStr,
}

impl parse::Parse for PathImplInput {
    fn parse(input: parse::ParseStream<'_>) -> parse::Result<Self> {
        Ok(Self {
            module: input.parse()?,
            comma: input.parse()?,
            path: input.parse()?,
        })
    }
}

pub fn path_impl(input: TokenStream) -> parse::Result<TokenStream> {
    let input: PathImplInput = syn::parse2(input)?;
    let path = &input.path.value();
    let output = PathImplOutput {
        path,
        params: parse_literal(path, input.path.span())?,
        module: input.module,
    };
    Ok(quote::quote_spanned!(input.path.span() => #output))
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Param<'a> {
    Single(&'a str),
    CatchAll(&'a str),
}

fn spanned_err<T>(span: Span, message: impl std::fmt::Display) -> parse::Result<T> {
    Err(parse::Error::new(span, message))
}

fn parse_literal(path: &str, span: Span) -> parse::Result<Vec<Param<'_>>> {
    match path {
        "" => return spanned_err(span, "the path cannot be empty"),
        "/" | "*" => return Ok(vec![]),
        _ => {}
    }

    let mut iter = path.split('/').peekable();
    if iter.next().map_or(false, |s| !s.is_empty()) {
        return spanned_err(span, "the path must start with a slash.");
    }

    let mut params = vec![];
    let mut names = HashSet::new();

    while let Some(segment) = iter.next() {
        match segment.split_at(1) {
            (":", name) => {
                if !names.insert(name) {
                    return spanned_err(
                        span,
                        format!("detected duplicate parameter name: '{}'", name),
                    );
                }
                params.push(Param::Single(name));
            }
            ("*", name) => {
                if !names.insert(name) {
                    return spanned_err(
                        span,
                        format!("detected duplicate parameter name: '{}'", name),
                    );
                }
                params.push(Param::CatchAll(name));
                break;
            }
            _ => {
                if segment.is_empty() && iter.peek().is_some() {
                    return spanned_err(span, "a segment must not be empty");
                }
            }
        }
    }

    if iter.next().is_some() {
        return spanned_err(span, "the catch-all parameter must be at the end of path");
    }

    Ok(params)
}

#[derive(Debug)]
pub struct PathImplOutput<'a> {
    module: syn::Path,
    path: &'a str,
    params: Vec<Param<'a>>,
}

impl<'a> ToTokens for PathImplOutput<'a> {
    #[allow(nonstandard_style)]
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let path = self.path;
        let module = &self.module;

        let Path = quote!(#module::Path);
        let PathExtractor = quote!(#module::PathExtractor);
        let Params = quote!(#module::Params);
        let PercentEncoded = quote!(#module::PercentEncoded);
        let FromPercentEncoded = quote!(#module::FromPercentEncoded);
        let Error = quote!(#module::Error);

        if self.params.is_empty() {
            tokens.append_all(quote!(
                fn call() -> #Path<()> {
                    #Path::new(#path)
                }
            ));
            return;
        }

        let type_idents: Vec<_> = self
            .params
            .iter()
            .enumerate()
            .map(|(i, _)| syn::Ident::new(&format!("T{}", i), Span::call_site()))
            .collect();
        let type_idents = &type_idents[..];

        let where_clause = {
            let bounds = type_idents
                .iter()
                .map(|ty| quote!(#ty: #FromPercentEncoded));
            quote!(where #(#bounds,)*)
        };
        let where_clause = &where_clause;

        let extract = self.params.iter().zip(type_idents).map(|(param, ty)| {
            let extract_raw = match param {
                Param::Single(name) => quote!(params.name(#name).expect("missing parameter")),
                Param::CatchAll(..) => {
                    quote!(params.catch_all().expect("missing catch-all parameter"))
                }
            };
            quote!(
                let #ty = <#ty as #FromPercentEncoded>::from_percent_encoded(
                    unsafe { #PercentEncoded::new_unchecked(#extract_raw) }
                ).map_err(Into::into)?;
            )
        });

        tokens.append_all(quote! {
            fn call<#(#type_idents),*>() -> #Path<impl #PathExtractor<Output = (#(#type_idents,)*)>>
            #where_clause
            {
                #[allow(missing_debug_implementations)]
                struct __Extractor<#(#type_idents),*> {
                    _marker: std::marker::PhantomData<fn() -> (#(#type_idents,)*)>,
                }

                impl<#(#type_idents),*> #PathExtractor for __Extractor<#(#type_idents),*>
                #where_clause
                {
                    type Output = (#(#type_idents,)*);

                    #[allow(nonstandard_style)]
                    fn extract(params: Option<&#Params<'_>>)
                        -> std::result::Result<Self::Output, #Error>
                    {
                        let params = params.expect("missing Params");
                        #( #extract )*
                        Ok((#(#type_idents,)*))
                    }
                }

                #Path::<__Extractor<#(#type_idents),*>>::new(#path)
            }
        });
    }
}
