//! The procedural macros for Tsukuyomi.

#![recursion_limit = "256"]
#![deny(nonstandard_style, rust_2018_idioms, rust_2018_compatibility, unused)]
#![forbid(clippy::unimplemented)]

extern crate proc_macro;

mod derive_into_response;
mod path_impl;

use proc_macro::TokenStream;

/// A procedural macro for deriving the implementation of `IntoResponse`.
///
/// # Example
///
/// If the custom derive applied to a struct with one field, the implementation
/// that calls the method `IntoResponse::into_response` of its field.
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// #[derive(IntoResponse)]
/// struct Foo(String);
///
/// #[derive(IntoResponse)]
/// struct Bar {
///     name: String,
/// }
///
/// #[derive(IntoResponse)]
/// struct Generic<T> {
///     inner: T,
/// }
/// ```
///
/// An example that applies the derivation to an enum:
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// #[derive(IntoResponse)]
/// enum Either<L, R> {
///     Left(L),
///     Right(R),
/// }
/// ```
///
/// It is possible to explicitly specify the function to create the HTTP response
/// by passing the path to function with `with = ".."` in the attribute `response`.
///
/// ```
/// # use tsukuyomi::IntoResponse;
/// # use tsukuyomi::vendor::http::{Request, Response};
/// # use tsukuyomi::util::Never;
/// use std::fmt::Debug;
///
/// #[derive(Debug, IntoResponse)]
/// #[response(
///     with = "into_response",
///     bound = "T: Debug",
///     bound = "U: Debug",
/// )]
/// struct CustomValue<T, U> {
///     t: T,
///     u: U,
/// }
///
/// fn into_response(
///     t: impl Debug,
///     request: &Request<()>,
/// ) -> Result<Response<String>, Never> {
///     // ...
/// #   unimplemented!()
/// }
/// # fn main() {}
/// ```
///
/// # Restrictions
/// Without specifying `with = "..."`, the number of fields in the struct
/// or the number of fields of each variant inside of the enum must be
/// at most one.  This is because the field to call `IntoResponse::into_response`
/// will not be determined if there are two or more fields in a struct or a variant.
///
/// ```compile_fail
/// # use tsukuyomi::IntoResponse;
/// #[derive(IntoResponse)]
/// struct MultiField {
///     title: String,
///     text: String,
/// }
/// ```
#[proc_macro_derive(IntoResponse, attributes(response))]
#[allow(nonstandard_style)]
#[cfg_attr(tarpaulin, skip)]
pub fn IntoResponse(input: TokenStream) -> TokenStream {
    crate::derive_into_response::derive_into_response(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

#[proc_macro]
pub fn path_impl(input: TokenStream) -> TokenStream {
    crate::path_impl::path_impl(input.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
