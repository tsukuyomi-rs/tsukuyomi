extern crate tsukuyomi;

use tsukuyomi::error::Result;
use tsukuyomi::extract::body::Json;
use tsukuyomi::extract::param::Param;
use tsukuyomi::handler::Handler;

fn assert_impl<T: Handler>(handler: T) {
    drop(handler);
}

#[tsukuyomi::handler::handler]
fn welcome() -> &'static str {
    "hello"
}

#[tsukuyomi::handler::future_handler]
fn extract_params(Param(p1): Param<i32>, Param(p2): Param<String>) -> Result<String> {
    Ok(format!("{}{}", p1, p2))
}

#[tsukuyomi::handler::future_handler]
fn read_json(body: Json<String>) -> Result<String> {
    Ok(body.0)
}

#[test]
#[ignore]
fn test_handler_macro() {
    assert_impl(welcome);
    assert_impl(extract_params);
    assert_impl(read_json);
}
