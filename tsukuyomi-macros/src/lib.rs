extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::*;

mod parsing {
    use syn::parse::{Parse, ParseStream, Result};

    #[derive(Debug)]
    pub struct RouteInput {
        pub method: syn::Ident,
        pub uri: syn::LitStr,
        _priv: (),
    }

    impl Parse for RouteInput {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(RouteInput {
                method: input.parse()?,
                uri: input.parse()?,
                _priv: (),
            })
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Component<'a> {
        Slash,
        Static(&'a str),
        Param(&'a str, &'a str, ParamKind),
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum ParamKind {
        Normal,
        Wildcard,
    }

    #[derive(Debug)]
    pub struct Components<'a> {
        uri: &'a str,
    }

    impl<'a> Iterator for Components<'a> {
        type Item = Component<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.uri.as_bytes().get(0) {
                Some(b'/') => {
                    self.uri = &self.uri[1..];
                    Some(Component::Slash)
                }
                Some(b'<') => {
                    let colon = self.uri.find(':').expect("invalid syntax: '>' not found");
                    let end = self.uri.find('>').expect("invalid syntax: '>' not found");
                    assert!(colon < end);

                    let mut name = &self.uri[1..colon];
                    let kind = if name.ends_with("..") {
                        name = &name[..name.len() - 2];
                        ParamKind::Wildcard
                    } else {
                        ParamKind::Normal
                    };
                    let ty = &self.uri[colon + 1..end];
                    self.uri = &self.uri[end + 1..];
                    Some(Component::Param(name, ty, kind))
                }
                Some(_) => {
                    let pos = self.uri.find('/').unwrap_or_else(|| self.uri.len());
                    let (static_, remains) = self.uri.split_at(pos);
                    self.uri = remains;
                    Some(Component::Static(static_))
                }
                None => None,
            }
        }
    }

    pub fn components(uri: &str) -> Components<'_> {
        Components { uri }
    }

    #[cfg(test)]
    mod tests {
        use super::components;
        use super::Component::*;
        use super::ParamKind::*;

        #[test]
        fn root() {
            assert_eq!(components("/").collect::<Vec<_>>(), vec![Slash,]);
        }

        #[test]
        fn static_() {
            assert_eq!(
                components("/path/to").collect::<Vec<_>>(),
                vec![Slash, Static("path"), Slash, Static("to"),]
            );
        }

        #[test]
        fn with_trailing_slash() {
            assert_eq!(
                components("/path/to/").collect::<Vec<_>>(),
                vec![Slash, Static("path"), Slash, Static("to"), Slash,]
            );
        }

        #[test]
        fn with_param() {
            assert_eq!(
                components("/path/to/<id:u32>").collect::<Vec<_>>(),
                vec![
                    Slash,
                    Static("path"),
                    Slash,
                    Static("to"),
                    Slash,
                    Param("id", "u32", Normal),
                ]
            );
        }

        #[test]
        fn with_param_and_trailing_slash() {
            assert_eq!(
                components("/path/to/<id:u32>/").collect::<Vec<_>>(),
                vec![
                    Slash,
                    Static("path"),
                    Slash,
                    Static("to"),
                    Slash,
                    Param("id", "u32", Normal),
                    Slash,
                ]
            );
        }

        #[test]
        fn multi_params() {
            assert_eq!(
                components("/path/to/<id:u32>/<name:std::string::String>").collect::<Vec<_>>(),
                vec![
                    Slash,
                    Static("path"),
                    Slash,
                    Static("to"),
                    Slash,
                    Param("id", "u32", Normal),
                    Slash,
                    Param("name", "std::string::String", Normal),
                ]
            );
        }

        #[test]
        fn wildcard() {
            assert_eq!(
                components("/<path..:PathBuf>").collect::<Vec<_>>(),
                vec![Slash, Param("path", "PathBuf", Wildcard),]
            );
        }
    }
}

#[proc_macro]
#[allow(nonstandard_style)]
pub fn route_impl(input: TokenStream) -> TokenStream {
    let input: parsing::RouteInput = match syn::parse(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error().into(),
    };

    let uri_str = input.uri.value();

    let mut params = vec![];
    let mut types = vec![];
    let mut generated_uri = String::new();

    for component in parsing::components(&uri_str) {
        use parsing::Component::*;
        use parsing::ParamKind::*;
        match component {
            Slash => generated_uri.push_str("/"),
            Static(s) => generated_uri.push_str(s),
            Param(name, ty, kind) => {
                params.push((name, kind));
                types.push(ty);
                generated_uri.push_str(&match kind {
                    Normal => format!(":{}", name),
                    Wildcard => format!("*{}", name),
                })
            }
        }
    }

    let Extractor: syn::Path = syn::parse_quote!(tsukuyomi::extractor::Extractor);
    let extractor: syn::Path = syn::parse_quote!(tsukuyomi::extractor);
    let Route: syn::Path = syn::parse_quote!(tsukuyomi::route::Route);
    let route: syn::Path = syn::parse_quote!(tsukuyomi::route);

    let extractors = params
        .into_iter()
        .enumerate()
        .map(|(i, (_name, kind))| -> syn::Expr {
            use parsing::ParamKind::*;
            match kind {
                Normal => syn::parse_quote!(#extractor::param::pos(#i)),
                Wildcard => syn::parse_quote!(#extractor::param::wildcard()),
            }
        });

    let types = types
        .into_iter()
        .map(|ty| syn::parse_str::<syn::Type>(ty).expect("invalid type in URI"));

    let tokens = quote! {
        fn route() -> #Route<impl #Extractor<Output = (#( #types, )*)>> {
            #route::get(#generated_uri)
                #( .with( #extractors ) )*
        }
    };

    tokens.into()
}
