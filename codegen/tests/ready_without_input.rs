#![feature(use_extern_macros)]

extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use tsukuyomi::Handler;
use tsukuyomi_codegen::handler;

fn assert_impl<T: Handler>(t: T) {
    drop(t);
}

#[handler]
fn handler() -> &'static str {
    "Hello"
}

#[test]
fn main() {
    assert_impl(handler);
}
