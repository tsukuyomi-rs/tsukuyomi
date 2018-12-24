//! The procedural macros for Tsukuyomi.

#![recursion_limit = "256"]
#![deny(nonstandard_style, rust_2018_idioms, rust_2018_compatibility, unused)]
#![cfg_attr(test, deny(warnings))]
#![forbid(clippy::unimplemented)]

extern crate proc_macro;

mod derive_into_response;
mod path_impl;

use proc_macro::TokenStream;

/// A procedural macro for deriving the implementation of `IntoResponse`.
///
/// # Examples
///
/// This macro has a parameter `#[response(preset = "..")]`, which specifies
/// the path to a type that implements a trait [`Preset`]:
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// use serde::Serialize;
///
/// #[derive(Debug, Serialize, IntoResponse)]
/// #[response(preset = "tsukuyomi::output::preset::Json")]
/// struct Post {
///     title: String,
///     text: String,
/// }
/// # fn main() {}
/// ```
///
/// You can specify the additional trait bounds to type parameters
/// by using the parameter `#[response(bound = "..")]`:
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// # use serde::Serialize;
/// #[derive(Debug, IntoResponse)]
/// #[response(
///     preset = "tsukuyomi::output::preset::Json",
///     bound = "T: Serialize",
///     bound = "U: Serialize",
/// )]
/// struct CustomValue<T, U> {
///     t: T,
///     u: U,
/// }
/// # fn main() {}
/// ```
///
/// # Notes
/// 1. When `preset = ".."` is omitted for struct, a field in the specified
///    struct is chosen and the the implementation of `IntoResponse` for its
///    type is used. For example, the impls derived to the following types
///    outputs eventually the same result as the implementation of
///    `IntoResponse` for `String`:
///    ```
///    # use tsukuyomi::IntoResponse;
///    #[derive(IntoResponse)]
///    struct Foo(String);
///   
///    #[derive(IntoResponse)]
///    struct Bar {
///        inner: String,
///    }
///    ```
/// 1. When `preset = ".."` is omitted for enum, the same rule as struct is
///    applied to each variant:
///    ```
///    # use tsukuyomi::IntoResponse;
///    # use tsukuyomi::vendor::http::Response;
///    #[derive(IntoResponse)]
///    enum MyResponse {
///        Text(String),
///        Raw { response: Response<String> },
///    }
///    ```
/// 1. Without specifying the preset, the number of fields in the struct
///    or the number of fields of each variant inside of the enum must be
///    at most one.  This is because the field that implements `IntoResponse`
///    cannot be determined if there are two or more fields in a struct or
///    a variant:
///    ```compile_fail
///    # use tsukuyomi::IntoResponse;
///    #[derive(IntoResponse)]
///    enum ApiResponse {
///        Text(String),
///        Post { title: String, text: String },
///    }
///    ```
///    If you want to apply the derivation to complex enums,
///    consider cutting each variant into one struct and specifying
///    the preset explicitly as follows:
///    ```
///    # use tsukuyomi::IntoResponse;
///    # use serde::Serialize;
///    #[derive(IntoResponse)]
///    enum ApiResponse {
///        Text(String),
///        Post(Post),
///    }
///
///    #[derive(Debug, Serialize, IntoResponse)]
///    #[response(preset = "tsukuyomi::output::preset::Json")]
///    struct Post {
///         title: String,
///         text: String,
///    }
///    ```
///
/// [`Preset`]: https://tsukuyomi-rs.github.io/tsukuyomi/tsukuyomi/output/preset/trait.Preset.html
#[proc_macro_derive(IntoResponse, attributes(response))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn IntoResponse(input: TokenStream) -> TokenStream {
    crate::derive_into_response::derive(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

#[proc_macro]
pub fn path_impl(input: TokenStream) -> TokenStream {
    crate::path_impl::path_impl(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
