extern crate cargo_version_sync;
extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_session;

use http::Request;
use tsukuyomi_session::{backend::CookieBackend, Session, SessionStorage};

#[test]
fn test_version_sync() {
    cargo_version_sync::assert_version_sync();
}

#[test]
fn smoketest() -> tsukuyomi::test::Result<()> {
    let backend = CookieBackend::plain().cookie_name("session");
    let storage = SessionStorage::new(backend);

    let mut server = tsukuyomi::app!()
        .modifier(storage)
        .route(
            tsukuyomi::route!("/counter", method = GET)
                .with(tsukuyomi_session::session())
                .handle(|sess: Session| -> tsukuyomi::Result<_> {
                    let counter: Option<i64> = sess.get("counter")?;
                    Ok(sess.finish(format!("{:?}", counter)))
                }),
        ) //
        .route(
            tsukuyomi::route!("/counter", method = PUT)
                .with(tsukuyomi_session::session())
                .handle(|mut sess: Session| -> tsukuyomi::Result<_> {
                    let counter: i64 = sess.get("counter")?.unwrap_or_default();
                    sess.set("counter", counter + 1)?;
                    Ok(sess.finish(format!("{}", counter)))
                }),
        ) //
        .route(
            tsukuyomi::route!("/counter", method = DELETE)
                .with(tsukuyomi_session::session())
                .reply(|mut sess: Session| {
                    sess.remove("counter");
                    sess.finish("removed")
                }),
        ) //
        .route(
            tsukuyomi::route!("/clear", method = PUT)
                .with(tsukuyomi_session::session())
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
