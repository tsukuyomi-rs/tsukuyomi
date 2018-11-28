extern crate cargo_version_sync;
extern crate http;
extern crate juniper;
extern crate percent_encoding;
extern crate tsukuyomi;
extern crate tsukuyomi_juniper;

#[test]
fn test_version_sync() {
    cargo_version_sync::assert_version_sync();
}

use {
    http::{Request, Response},
    juniper::{http::tests as http_tests, tests::model::Database, EmptyMutation, RootNode},
    percent_encoding::{define_encode_set, utf8_percent_encode, QUERY_ENCODE_SET},
    std::cell::RefCell,
    tsukuyomi::test::{Output as TestOutput, Server as TestServer},
    tsukuyomi_juniper::Executor,
};

#[test]
fn integration_test() -> tsukuyomi::test::Result<()> {
    let database = std::sync::Arc::new(Database::new());
    let schema = RootNode::new(Database::new(), EmptyMutation::<Database>::new());
    let executor = tsukuyomi_juniper::executor(schema);
    let executor = std::sync::Arc::new(executor);

    let test_server = tsukuyomi::app!()
        .with(
            tsukuyomi::app::route!("/", methods = [GET, POST])
                .extract(executor.clone())
                .call({
                    let database = database.clone();
                    move |exec: Executor<_>| exec.execute(database.clone())
                }),
        ) //
        .build_server()?
        .into_test_server()?;

    let integration = TestTsukuyomiIntegration {
        local_server: RefCell::new(test_server),
    };

    http_tests::run_http_test_suite(&integration);

    Ok(())
}

struct TestTsukuyomiIntegration {
    local_server: RefCell<TestServer<tsukuyomi::app::App>>,
}

impl http_tests::HTTPIntegration for TestTsukuyomiIntegration {
    fn get(&self, url: &str) -> http_tests::TestResponse {
        let response = self
            .local_server
            .borrow_mut()
            .perform(Request::get(custom_url_encode(url)))
            .unwrap();
        make_test_response(&response)
    }

    fn post(&self, url: &str, body: &str) -> http_tests::TestResponse {
        let response = self
            .local_server
            .borrow_mut()
            .perform(
                Request::post(custom_url_encode(url))
                    .header("content-type", "application/json")
                    .body(body),
            ).unwrap();
        make_test_response(&response)
    }
}

fn custom_url_encode(url: &str) -> String {
    define_encode_set! {
        pub CUSTOM_ENCODE_SET = [QUERY_ENCODE_SET] | {'{', '}'}
    }
    utf8_percent_encode(url, CUSTOM_ENCODE_SET).to_string()
}

#[cfg_attr(feature = "cargo-clippy", allow(cast_lossless))]
fn make_test_response(response: &Response<TestOutput>) -> http_tests::TestResponse {
    let status_code = response.status().as_u16() as i32;
    let content_type = response
        .headers()
        .get("content-type")
        .expect("missing Content-type")
        .to_str()
        .expect("Content-type should be a valid UTF-8 string")
        .to_owned();
    let body = response
        .body()
        .to_utf8()
        .expect("The response body should be a valid UTF-8 string")
        .into_owned();
    http_tests::TestResponse {
        status_code,
        content_type,
        body: Some(body),
    }
}
