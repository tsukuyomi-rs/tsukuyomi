use {
    askama::Template,
    tsukuyomi::{
        config::prelude::*, //
        test::ResponseExt,
        App,
        IntoResponse,
    },
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_template_with_derivation_responder() -> tsukuyomi::test::Result<()> {
    #[derive(Template, IntoResponse)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[response(with = "tsukuyomi_askama::into_response")]
    struct Index {
        name: &'static str,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| Index { name: "Alice" })),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}

#[test]
fn test_template_with_modifier() -> tsukuyomi::test::Result<()> {
    #[derive(Template)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    struct Index {
        name: &'static str,
    }

    let app = App::create(
        path!("/") //
            .to(endpoint::get() //
                .call(|| Index { name: "Alice" }))
            .modify(tsukuyomi_askama::Renderer::default()),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}
