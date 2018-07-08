#![feature(proc_macro)]

extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::{future, Future};
use tsukuyomi::{Error, Handler, Input};
use tsukuyomi_codegen::handler;

#[handler(async)]
fn handler(_: &mut Input) -> impl Future<Item = &'static str, Error = Error> + Send + 'static {
    future::ok("Hello")
}

#[test]
fn main() {
    let _ = Handler::new(handler);
}
