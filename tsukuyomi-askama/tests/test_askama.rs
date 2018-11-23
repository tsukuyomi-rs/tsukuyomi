extern crate askama;
extern crate cargo_version_sync;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;
use tsukuyomi::output::Responder;
use tsukuyomi::test::ResponseExt;

#[test]
fn version_sync() {
    cargo_version_sync::assert_version_sync();
}

#[inline]
fn assert_impl<T: Responder>(x: T) -> T {
    x
}

#[test]
fn test_template() -> tsukuyomi::test::Result<()> {
    #[derive(Template, Responder)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[responder(respond_to = "tsukuyomi_askama::respond_to")]
    struct Index {
        name: &'static str,
    }

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::app::route!("/") //
                .reply(|| assert_impl(Index { name: "Alice" })),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}
