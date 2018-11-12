extern crate cookie;
extern crate either;
extern crate futures;
extern crate http;
extern crate serde;
extern crate time;
extern crate tsukuyomi;

macro_rules! try_expr {
    ($body:expr) => {{
        #[cfg_attr(feature = "cargo-clippy", allow(redundant_closure_call))]
        (|| $body)()
    }};
}

mod integration_tests;
