macro_rules! decl_attribute {
    (
        $(#[$m:meta])*
        fn $name:ident($item:ident : $t:ty) -> $ret:ty {
            $($bd:stmt)*
        }
    ) => {
        $(#[$m])*
        #[proc_macro_attribute]
        pub fn $name(_: proc_macro::TokenStream, item: proc_macro::TokenStream)
            -> proc_macro::TokenStream
        {
            fn inner($item: $t) -> $ret {
                $($bd)*
            }

            let item: $t = match syn::parse(item) {
                Ok(item) => item,
                Err(err) => return err.to_compile_error().into(),
            };
            let result: $ret = inner(item);
            quote::quote!(#result).into()
        }
    };
}
