use {
    crate::uri::Uri,
    proc_macro2::TokenStream,
    syn::{
        parse::{Error as ParseError, Result as ParseResult},
        spanned::Spanned,
    },
};

pub fn validate(input: TokenStream) -> ParseResult<()> {
    let uri_span = input.span();
    let uri_lit: syn::LitStr = syn::parse2(input)?;

    let uri: Uri = uri_lit
        .value()
        .parse()
        .map_err(|err| ParseError::new(uri_span, format!("URI parse error: {}", err)))?;

    if uri.capture_names().is_some() {
        return Err(syn::parse::Error::new(
            uri_span,
            "parameters canoot be used in the prefix position",
        ));
    }

    if uri.is_asterisk() {
        return Err(syn::parse::Error::new(
            uri_span,
            "the asterisk URI cannot be used as the prefix",
        ));
    }

    Ok(())
}
