use proc_macro2::TokenStream;
use synstructure::Structure;

pub fn derive_local_data(s: Structure) -> TokenStream {
    let ident = &s.ast().ident;

    quote::quote!{
        use tsukuyomi::input::local_map::local_key;
        impl tsukuyomi::input::local_map::LocalData for #ident {
            local_key!(const KEY: Self);
        }
    }
}
