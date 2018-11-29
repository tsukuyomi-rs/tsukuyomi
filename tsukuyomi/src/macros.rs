/// A macro to start building an `App`.
#[deprecated(
    since = "0.4.2",
    note = "this macro will be removed in the next version."
)]
#[macro_export(local_inner_macros)]
macro_rules! app {
    () => {
        $crate::app::app()
    };
    ($prefix:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            validate_prefix!($prefix);
        }
        $crate::app::app().prefix($prefix.parse().expect("this is a bug"))
    }};
}

/// A macro to start building a `Scope`.
#[macro_export(local_inner_macros)]
#[deprecated(
    since = "0.4.2",
    note = "this macro will be removed in the next version."
)]
macro_rules! scope {
    () => {
        $crate::app::scope()
    };

    ($prefix:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            validate_prefix!($prefix);
        }
        $crate::app::scope().prefix($prefix.parse().expect("this is a bug"))
    }};
}

pub mod route {
    pub use {
        crate::app::scope::route, //
        http::Method,
    };
}

/// A macro to start building a `Route`.
#[macro_export(local_inner_macros)]
macro_rules! route {
    () => ( $crate::macros::route::route("/").expect("this is a bug") );
    ($uri:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            route_expr_impl!($uri);
        }
        __Dummy::route()
    }};
    ($uri:expr, method = $METHOD:ident) => {{
        #[allow(nonstandard_style)]
        enum __priv__ {}
        impl __priv__ {
            #[inline(always)]
            #[deprecated(since = "0.4.2", note = "the option `method = $METHOD` is deprecated and will be removed in the next version. In order to specify the methods, use `Route::methods` instead")]
            fn deprecation() {}
        }
        __priv__::deprecation();

        route!($uri)
            .methods($crate::macros::route::Method::$METHOD)
            .expect("should be valid")
    }};
    ($uri:expr, methods = [$($METHODS:ident),*]) => {{
        #[allow(nonstandard_style)]
        enum __priv__ {}
        impl __priv__ {
            #[inline(always)]
            #[deprecated(since = "0.4.2", note = "the option `methods = [$($METHODS),*]` is deprecated and will be removed in the next version. In order to specify the methods, use `Route::methods` instead")]
            fn deprecation() {}
        }
        __priv__::deprecation();

        route!($uri)
            .methods(__tsukuyomi_vec![$($crate::macros::route::Method::$METHODS),*])
            .expect("should be valid")
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tsukuyomi_vec {
    ($($t:tt)*) => (vec![$($t)*]);
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tsukuyomi_compile_error {
    ($($t:tt)*) => { compile_error! { $($t)* } };
}
