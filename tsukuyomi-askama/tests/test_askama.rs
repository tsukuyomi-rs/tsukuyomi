use {
    askama::Template,
    http::{header::CONTENT_TYPE, StatusCode},
    tsukuyomi::{
        endpoint,
        test::{self, loc, TestServer},
        App, Responder,
    },
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_template_derivation() -> test::Result {
    #[derive(Template, Responder)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    #[response(preset = "tsukuyomi_askama::Askama")]
    struct Index {
        name: &'static str,
    }

    let app = App::build(|mut s| {
        s.at("/")?
            .get()
            .to(endpoint::call(|| Index { name: "Alice" }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/")
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(CONTENT_TYPE, "text/html"))?
        .assert(loc!(), test::body::eq("Hello, Alice."))?;

    Ok(())
}

#[test]
fn test_template_with_modifier() -> test::Result {
    #[derive(Template)]
    #[template(source = "Hello, {{ name }}.", ext = "html")]
    struct Index {
        name: &'static str,
    }

    let app = App::build(|mut s| {
        s.at("/")?
            .get()
            .with(tsukuyomi_askama::renderer())
            .to(endpoint::call(|| Index { name: "Alice" }))
    })?;

    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .get("/")
        .assert(loc!(), StatusCode::OK)?
        .assert(loc!(), test::header::eq(CONTENT_TYPE, "text/html"))?
        .assert(loc!(), test::body::eq("Hello, Alice."))?;

    Ok(())
}
