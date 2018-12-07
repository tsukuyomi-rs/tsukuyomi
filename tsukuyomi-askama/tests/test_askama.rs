use {
    askama::Template,
    tsukuyomi::{
        app::{route, App},
        output::Responder,
        test::ResponseExt,
    },
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_template_with_derivation_responder() -> tsukuyomi::test::Result<()> {
    #[derive(Template, Responder)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[responder(respond_to = "tsukuyomi_askama::respond_to")]
    struct Index {
        name: &'static str,
    }

    let mut server = App::builder()
        .with(route::root().reply(|| Index { name: "Alice" })) //
        .build_server()?
        .into_test_server()?;

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

    let mut server = App::builder()
        .modifier(tsukuyomi_askama::Renderer::default())
        .with(route::root().reply(|| Index { name: "Alice" })) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform("/")?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header("content-type")?, "text/html");
    assert_eq!(response.body().to_utf8()?, "Hello, Alice.");

    Ok(())
}
