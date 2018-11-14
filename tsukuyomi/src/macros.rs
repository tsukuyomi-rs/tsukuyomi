pub mod local_key {
    pub use crate::input::local_map::LocalKey;
    pub use std::any::TypeId;
    pub use std::marker::PhantomData;
}

/// A macro to create a `LocalKey<T>`.
#[macro_export]
macro_rules! local_key {
    ($(#[$m:meta])* $vis:vis static $NAME:ident : $t:ty; $($tail:tt)*) => {
        local_key!(@declare ($vis) static $NAME: $t);
        local_key!($($tail)*);
    };

    ($(#[$m:meta])* $vis:vis const $NAME:ident : $t:ty; $($tail:tt)*) => {
        local_key!(@declare ($vis) const $NAME: $t);
        local_key!($($tail)*);
    };

    () => ();

    (@declare $(#[$m:meta])* ($($vis:tt)*) $kw:tt $NAME:ident : $t:ty) => {
        $(#[$m])*
        $($vis)* $kw $NAME: $crate::macros::local_key::LocalKey<$t> = {
            fn __type_id() -> $crate::macros::local_key::TypeId {
                struct __A;
                $crate::macros::local_key::TypeId::of::<__A>()
            }
            $crate::macros::local_key::LocalKey {
                __type_id,
                __marker: $crate::macros::local_key::PhantomData,
            }
        };
    };
}

#[macro_export(local_inner_macros)]
macro_rules! app {
    () => {
        $crate::app()
    };
    ($prefix:expr) => {{
        enum __Dummy {}
        impl __Dummy {
            validate_prefix!($prefix);
        }
        $crate::app().prefix($prefix.parse().expect("this is a bug"))
    }};
}

#[macro_export(local_inner_macros)]
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
    pub use crate::app::route;
    pub use http::Method;
}

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
