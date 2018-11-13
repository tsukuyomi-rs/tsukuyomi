extern crate askama;
extern crate cargo_version_sync;
extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;
use http::Request;
use tsukuyomi::output::Responder;
use tsukuyomi::test::test_server;

#[test]
fn version_sync() {
    cargo_version_sync::assert_version_sync();
}

#[inline]
fn assert_impl<T: Responder>(x: T) -> T {
    x
}

#[test]
fn test_template() {
    #[derive(Template, Responder)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[responder(respond_to = "tsukuyomi_askama::respond_to")]
    struct Index {
        name: &'static str,
    }

    let mut server = test_server(
        tsukuyomi::app()
            .route(
                tsukuyomi::app::route!("/") //
                    .reply(|| assert_impl(Index { name: "Alice" })),
            ) //
            .finish()
            .unwrap(),
    );

    let response = server.perform(Request::get("/")).unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/html");
    assert_eq!(response.body().to_utf8().unwrap(), "Hello, Alice.");
}
