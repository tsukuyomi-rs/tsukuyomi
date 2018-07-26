#![feature(use_extern_macros)]
#![feature(proc_macro_non_items, generators)]

extern crate futures_await as futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::future;
use futures::prelude::await;
use tsukuyomi::{Error, Handler, Input};
use tsukuyomi_codegen::handler;

fn assert_impl<T: Handler>(t: T) {
    drop(t);
}

#[handler]
fn handler(_: &mut Input) -> &'static str {
    "Hello"
}

#[handler(async)]
fn handler_with_await() -> tsukuyomi::Result<&'static str> {
    await!(future::ok::<(), Error>(()))?;
    Ok("Hello")
}

#[test]
fn main() {
    assert_impl(handler);
    assert_impl(handler_with_await);
}
