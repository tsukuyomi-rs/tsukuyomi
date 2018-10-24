extern crate tsukuyomi;

use tsukuyomi::error::Result;
use tsukuyomi::handler::Handler;
use tsukuyomi::input::body::Json;
use tsukuyomi::input::param::Param;

fn assert_impl<T: Handler>(handler: T) {
    drop(handler);
}

tsukuyomi::handler! {
    fn welcome() -> Result<&'static str> {
        Ok("hello")
    }
}

tsukuyomi::handler! {
    fn extract_params(p1: Param<i32>, p2: Param<String>) -> Result<String> {
        Ok(format!("{}{}", &*p1, &*p2))
    }
}

tsukuyomi::handler! {
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
