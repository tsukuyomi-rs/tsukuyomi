extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use tsukuyomi::{Handler, Input};
use tsukuyomi_codegen::handler;

fn assert_impl<T: Handler>(t: T) {
    drop(t);
}

#[handler]
fn handler(_: &mut Input) -> &'static str {
    "Hello"
}

// FIXME: re-enable
// #[handler(async)]
// fn handler_with_await() -> tsukuyomi::Result<&'static str> {
//     await!(future::ok::<(), Error>(()))?;
//     Ok("Hello")
// }

#[test]
fn main() {
    assert_impl(handler);
    // assert_impl(handler_with_await);
}
