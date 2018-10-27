extern crate futures;
extern crate tsukuyomi;

use futures::prelude::*;
use tsukuyomi::error::Error;
use tsukuyomi::extract::body::Json;
use tsukuyomi::extract::param::Param;
use tsukuyomi::handler::Handler;

fn assert_impl<T: Handler>(handler: T) {
    drop(handler);
}

#[tsukuyomi::handler::extract_ready]
fn welcome() -> &'static str {
    "hello"
}

#[tsukuyomi::handler::extract_ready]
fn extract_params(Param(p1): Param<i32>, Param(p2): Param<String>) -> String {
    format!("{}{}", p1, p2)
}

#[tsukuyomi::handler::extract]
fn read_json(body: Json<String>) -> impl Future<Error = Error, Item = String> {
    futures::future::ok(body.0)
}

#[test]
#[ignore]
fn test_handler_macro() {
    assert_impl(welcome);
    assert_impl(extract_params);
    assert_impl(read_json);
}
