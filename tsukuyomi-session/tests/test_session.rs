extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_session;
extern crate version_sync;

use {
    http::Request,
    tsukuyomi::app::directives::*,
    tsukuyomi_session::{
        backend::CookieBackend, //
        session,
        Session,
        SessionStorage,
    },
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn smoketest() -> tsukuyomi::test::Result<()> {
    let backend = CookieBackend::plain().cookie_name("session");
    let storage = SessionStorage::new(backend);

    let mut server = App::builder()
        .with(modifier(storage))
        .with(route!("/counter").methods("GET")?.extract(session()).call(
            |sess: Session| -> tsukuyomi::Result<_> {
                let counter: Option<i64> = sess.get("counter")?;
                Ok(sess.finish(format!("{:?}", counter)))
            },
        )) //
        .with(route!("/counter").methods("PUT")?.extract(session()).call(
            |mut sess: Session| -> tsukuyomi::Result<_> {
                let counter: i64 = sess.get("counter")?.unwrap_or_default();
                sess.set("counter", counter + 1)?;
                Ok(sess.finish(format!("{}", counter)))
            },
        )) //
        .with(
            route!("/counter")
                .methods("DELETE")?
                .extract(session())
                .reply(|mut sess: Session| {
                    sess.remove("counter");
                    sess.finish("removed")
                }),
        ) //
        .with(
            route!("/clear")
                .methods("PUT")?
                .extract(session())
                .reply(|mut sess: Session| {
                    sess.clear();
                    sess.finish("cleared")
                }),
        ) //
        .build_server()?
        .into_test_server()?;

    let mut session = server.new_session()?.save_cookies(true);

    let response = session.perform(Request::get("/counter"))?;
    assert!(!response.headers().contains_key("set-cookie"));
    assert_eq!(response.body().to_utf8()?, "None");

    let response = session.perform(Request::put("/counter"))?;
    assert!(response.headers().contains_key("set-cookie"));

    let response = session.perform(Request::get("/counter"))?;
    assert!(response.headers().contains_key("set-cookie"));
    assert_eq!(response.body().to_utf8()?, "Some(1)");

    let response = session.perform(Request::put("/counter"))?;
    assert!(response.headers().contains_key("set-cookie"));
    assert_eq!(response.body().to_utf8()?, "1");

    session.perform(Request::delete("/counter"))?;
    assert!(session.cookie("session").is_some());
    assert_eq!(session.perform("/counter")?.body().to_utf8()?, "None");

    session.perform(Request::put("/clear"))?;
    assert!(session.cookie("session").is_none());

    Ok(())
}
