use {
    http::Request,
    tsukuyomi::{config::prelude::*, App},
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
fn smoketest() -> tsukuyomi_server::Result<()> {
    let backend = CookieBackend::plain().cookie_name("session");
    let session = std::sync::Arc::new(session(backend));

    let app = App::create(chain![
        path!("/counter").to(chain![
            endpoint::get() //
                .extract(session.clone())
                .call_async(|session: Session<_>| -> tsukuyomi::Result<_> {
                    let counter: Option<i64> = session.get("counter")?;
                    Ok(session.finish(format!("{:?}", counter)))
                }),
            endpoint::put() //
                .extract(session.clone())
                .call_async(|mut session: Session<_>| -> tsukuyomi::Result<_> {
                    let counter: i64 = session.get("counter")?.unwrap_or_default();
                    session.set("counter", counter + 1)?;
                    Ok(session.finish(format!("{}", counter)))
                }),
            endpoint::delete() //
                .extract(session.clone())
                .call(|mut session: Session<_>| {
                    session.remove("counter");
                    session.finish("removed")
                }),
        ]),
        path!("/clear").to(endpoint::put()
            .extract(session)
            .call(|mut session: Session<_>| {
                session.clear();
                session.finish("cleared")
            }))
    ])?;

    let mut server = tsukuyomi_server::test::server(app.into_service())?;
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
