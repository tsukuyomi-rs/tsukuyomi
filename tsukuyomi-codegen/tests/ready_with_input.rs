#![feature(proc_macro)]

extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use tsukuyomi::{Handler, Input};
use tsukuyomi_codegen::handler;

#[handler]
fn handler(_: &mut Input) -> &'static str {
    "Hello"
}

#[test]
fn main() {
    let _ = Handler::new(handler);
}
