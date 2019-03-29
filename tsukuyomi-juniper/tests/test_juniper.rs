use {
    http::{Method, Request},
    juniper::{http::tests as http_tests, tests::model::Database, EmptyMutation, RootNode},
    percent_encoding::{define_encode_set, utf8_percent_encode, QUERY_ENCODE_SET},
    std::{cell::RefCell, sync::Arc},
    tsukuyomi::{
        endpoint,
        test::{self, TestResponse, TestServer},
        App,
    },
    tsukuyomi_juniper::GraphQLRequest,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn integration_test() -> test::Result {
    let database = Arc::new(Database::new());
    let schema = Arc::new(RootNode::new(
        Database::new(),
        EmptyMutation::<Database>::new(),
    ));

    let app = App::build(|mut s| {
        let database = database.clone();
        s.at("/")?
            .route(&[Method::GET, Method::POST])
            .with(tsukuyomi_juniper::capture_errors())
            .extract(tsukuyomi_juniper::request())
            .extract(tsukuyomi::extractor::value(schema))
            .to(endpoint::call(
                move |request: GraphQLRequest, schema: Arc<_>| {
                    let database = database.clone();
                    request.execute(schema, database)
                },
            ))
    })?;

    let test_server = TestServer::new(app)?;

    let integration = TestTsukuyomiIntegration {
        local_server: RefCell::new(test_server),
    };

    http_tests::run_http_test_suite(&integration);

    Ok(())
}

struct TestTsukuyomiIntegration {
    local_server: RefCell<TestServer>,
}

impl http_tests::HTTPIntegration for TestTsukuyomiIntegration {
    fn get(&self, url: &str) -> http_tests::TestResponse {
        let mut server = self.local_server.borrow_mut();
        let mut client = server.connect();
        let response = client.request(Request::get(custom_url_encode(url)).body("").unwrap());
        make_test_response(response)
    }

    fn post(&self, url: &str, body: &str) -> http_tests::TestResponse {
        let mut server = self.local_server.borrow_mut();
        let mut client = server.connect();
        let response = client.request(
            Request::post(custom_url_encode(url))
                .header("content-type", "application/json")
                .body(body.as_bytes())
                .unwrap(),
        );
        make_test_response(response)
    }
}

fn custom_url_encode(url: &str) -> String {
    define_encode_set! {
        pub CUSTOM_ENCODE_SET = [QUERY_ENCODE_SET] | {'{', '}'}
    }
    utf8_percent_encode(url, CUSTOM_ENCODE_SET).to_string()
}

#[allow(clippy::cast_lossless)]
fn make_test_response(response: TestResponse<'_>) -> http_tests::TestResponse {
    let status_code = response.status().as_u16() as i32;
    let content_type = response
        .headers()
        .get("content-type")
        .expect("missing Content-type")
        .to_str()
        .expect("Content-type should be a valid UTF-8 string")
        .to_owned();
    let body = String::from_utf8(response.into_bytes().unwrap())
        .expect("The response body should be a valid UTF-8 string");
    http_tests::TestResponse {
        status_code,
        content_type,
        body: Some(body),
    }
}
