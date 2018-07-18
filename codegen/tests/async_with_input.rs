#![feature(use_extern_macros)]

extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::{future, Future};
use tsukuyomi::{Error, Handler, Input};
use tsukuyomi_codegen::handler;

fn assert_impl<T: Handler>(t: T) {
    drop(t);
}

#[handler(async)]
fn handler(_: &mut Input) -> impl Future<Item = &'static str, Error = Error> + Send + 'static {
    future::ok("Hello")
}

#[test]
fn main() {
    assert_impl(handler);
}
