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
        .with(tsukuyomi::app::modifier(storage))
        .with(
            tsukuyomi::route!("/counter", method = GET)
                .extract(tsukuyomi_session::session())
                .call(|sess: Session| -> tsukuyomi::Result<_> {
                    let counter: Option<i64> = sess.get("counter")?;
                    Ok(sess.finish(format!("{:?}", counter)))
                }),
        ) //
        .with(
            tsukuyomi::route!("/counter", method = PUT)
                .extract(tsukuyomi_session::session())
                .call(|mut sess: Session| -> tsukuyomi::Result<_> {
                    let counter: i64 = sess.get("counter")?.unwrap_or_default();
                    sess.set("counter", counter + 1)?;
                    Ok(sess.finish(format!("{}", counter)))
                }),
        ) //
        .with(
            tsukuyomi::route!("/counter", method = DELETE)
                .extract(tsukuyomi_session::session())
                .reply(|mut sess: Session| {
                    sess.remove("counter");
                    sess.finish("removed")
                }),
        ) //
        .with(
            tsukuyomi::route!("/clear", method = PUT)
                .extract(tsukuyomi_session::session())
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
