#[doc(hidden)]
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

#[doc(hidden)]
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

#[allow(deprecated)]
pub mod route {
    pub use {
        crate::app::route, //
        http::Method,
    };
}

#[doc(hidden)]
#[deprecated(since = "0.4.2", note = "use `route2!()` instead")]
#[macro_export(local_inner_macros)]
macro_rules! route {
    () => ( $crate::macros::route::route() );
    ($uri:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            route_expr_impl!($uri);
        }
        __Dummy::route()
    }};
    ($uri:expr, method = $METHOD:ident) => {
        route!($uri).method($crate::macros::route::Method::$METHOD)
    };
    ($uri:expr, methods = [$($METHODS:ident),*]) => {
        route!($uri).methods(__tsukuyomi_vec![$($crate::macros::route::Method::$METHODS),*])
    };
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
