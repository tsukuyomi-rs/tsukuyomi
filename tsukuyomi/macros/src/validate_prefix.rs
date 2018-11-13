use crate::uri::Uri;

use proc_macro2::TokenStream;
use syn::spanned::Spanned;

pub fn validate(input: TokenStream) -> syn::parse::Result<()> {
    let uri_span = input.span();
    let uri_lit: syn::LitStr = syn::parse2(input)?;

    let uri: Uri = uri_lit
        .value()
        .parse()
        .map_err(|err| syn::parse::Error::new(uri_span, format!("URI parse error: {}", err)))?;

    if uri.capture_names().is_some() {
        return Err(syn::parse::Error::new(
            uri_span,
            "parameters in prefix position is forbidden",
        ));
    }

    Ok(())
}
