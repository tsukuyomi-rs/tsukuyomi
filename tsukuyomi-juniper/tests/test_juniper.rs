#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

use {
    http::{Request, Response},
    juniper::{http::tests as http_tests, tests::model::Database, EmptyMutation, RootNode},
    percent_encoding::{define_encode_set, utf8_percent_encode, QUERY_ENCODE_SET},
    std::{cell::RefCell, sync::Arc},
    tsukuyomi::{
        config::prelude::*,
        test::{Output as TestOutput, Server as TestServer},
        App,
    },
    tsukuyomi_juniper::{GraphQLModifier, GraphQLRequest},
};

#[test]
fn integration_test() -> tsukuyomi::test::Result<()> {
    let database = Arc::new(Database::new());
    let schema = Arc::new(RootNode::new(
        Database::new(),
        EmptyMutation::<Database>::new(),
    ));

    let app = App::create({
        let database = database.clone();
        path!("/")
            .to(endpoint::allow_only("GET, POST")?
                .extract(tsukuyomi_juniper::request())
                .extract(tsukuyomi::extractor::value(schema))
                .call(move |request: GraphQLRequest, schema: Arc<_>| {
                    let database = database.clone();
                    request.execute(schema, database)
                }))
            .modify(GraphQLModifier::default())
    })?;

    let test_server = tsukuyomi::test::server(app)?;

    let integration = TestTsukuyomiIntegration {
        local_server: RefCell::new(test_server),
    };

    http_tests::run_http_test_suite(&integration);

    Ok(())
}

struct TestTsukuyomiIntegration {
    local_server: RefCell<TestServer<tsukuyomi::App>>,
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
            )
            .unwrap();
        make_test_response(&response)
    }
}

fn custom_url_encode(url: &str) -> String {
    define_encode_set! {
        pub CUSTOM_ENCODE_SET = [QUERY_ENCODE_SET] | {'{', '}'}
    }
    utf8_percent_encode(url, CUSTOM_ENCODE_SET).to_string()
}

#[allow(clippy::cast_lossless)]
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
