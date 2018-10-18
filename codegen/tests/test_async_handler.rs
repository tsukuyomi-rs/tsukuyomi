#![cfg(feature = "nightly")]

extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use tsukuyomi::{AsyncResponder, Error, Handler, Input};
use tsukuyomi_codegen::async_handler;

fn assert_impl<T: Handler>(t: T) {
    drop(t);
}

#[async_handler]
fn handler(_: &mut Input) -> impl AsyncResponder {
    futures::future::ok::<_, Error>("Hello")
}

#[test]
fn main() {
    assert_impl(handler);
}
