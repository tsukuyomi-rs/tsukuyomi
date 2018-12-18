use {
    cookie::Cookie,
    tsukuyomi::{config::prelude::*, App},
};

#[test]
fn enable_manage_cookies() -> tsukuyomi_server::Result<()> {
    let app = App::create(chain![
        path!("/first").to(endpoint::any() //
            .reply(tsukuyomi::responder::oneshot(|input| {
                input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                Ok::<_, tsukuyomi::Error>("")
            }))),
        path!("/second").to(endpoint::any() //
            .reply(tsukuyomi::responder::oneshot(|input| {
                assert!(input.cookies.jar()?.get("session").is_some());
                Ok::<_, tsukuyomi::Error>("")
            }))),
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let mut session = server.new_session()?.save_cookies(true);
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}

#[test]
fn disable_manage_cookies() -> tsukuyomi_server::Result<()> {
    let app = App::create(chain![
        path!("/first") //
            .to(endpoint::any() //
                .reply(tsukuyomi::responder::oneshot(|input| {
                    input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                    Ok::<_, tsukuyomi::Error>("")
                }))),
        path!("/second") //
            .to(endpoint::any() //
                .reply(tsukuyomi::responder::oneshot(|input| {
                    assert!(input.cookies.jar()?.get("session").is_none());
                    Ok::<_, tsukuyomi::Error>("")
                }))),
    ])?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let mut session = server.new_session()?;
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}
