#![warn(
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate failure;
#[cfg_attr(test, macro_use)]
extern crate indexmap;

pub mod uri;
