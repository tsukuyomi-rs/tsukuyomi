#![feature(proc_macro)]

extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use tsukuyomi::Handler;
use tsukuyomi_codegen::handler;

#[handler]
fn handler() -> &'static str {
    "Hello"
}

#[test]
fn main() {
    let _ = Handler::new(handler);
}
