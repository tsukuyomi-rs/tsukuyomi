extern crate tsukuyomi;

use tsukuyomi::error::Result;
use tsukuyomi::handler::define_handler;
use tsukuyomi::handler::Handler;
use tsukuyomi::input::body::Json;
use tsukuyomi::input::param::Param;

fn assert_impl<T: Handler>(handler: T) {
    drop(handler);
}

define_handler! {
    @ready
    fn welcome() -> &'static str {
        "hello"
    }
}

define_handler! {
    fn extract_params(p1: Param<i32>, p2: Param<String>) -> Result<String> {
        Ok(format!("{}{}", &*p1, &*p2))
    }
}

define_handler! {
    fn read_json(body: Json<String>) -> Result<String> {
        Ok(body.0)
    }
}

#[test]
#[ignore]
fn test_handler_macro() {
    assert_impl(welcome);
    assert_impl(extract_params);
    assert_impl(read_json);
}
