extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_session;
extern crate version_sync;

use {
    http::Request,
    tsukuyomi::{app::config::prelude::*, chain, App},
    tsukuyomi_session::{
        backend::CookieBackend, //
        session,
        Session,
    },
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn smoketest() -> tsukuyomi::test::Result<()> {
    let backend = CookieBackend::plain().cookie_name("session");
    let session = std::sync::Arc::new(session(backend));

    let app = App::create(chain![
        path!(/"counter").extract(session.clone()).to(chain![
            endpoint::get().call_async(|session: Session<_>| {
                let counter: Option<i64> = session.get("counter")?;
                Ok::<_, tsukuyomi::Error>(session.finish(format!("{:?}", counter)))
            }),
            endpoint::put().call_async(|mut session: Session<_>| {
                let counter: i64 = session.get("counter")?.unwrap_or_default();
                session.set("counter", counter + 1)?;
                Ok::<_, tsukuyomi::Error>(session.finish(format!("{}", counter)))
            }),
            endpoint::delete().call(|mut session: Session<_>| {
                session.remove("counter");
                session.finish("removed")
            }),
        ]),
        path!(/"clear")
            .extract(session)
            .to(endpoint::put().call(|mut session: Session<_>| {
                session.clear();
                session.finish("cleared")
            }),)
    ])?;

    let mut server = tsukuyomi::test::server(app)?;
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
