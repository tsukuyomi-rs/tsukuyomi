extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;
extern crate version_sync;

use askama::Template;
use tsukuyomi::output::Responder;
use tsukuyomi::test::ResponseExt;

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
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
        .with(
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
