use {
    cookie::Cookie,
    tsukuyomi::{
        app::config::route::Route, //
        chain,
        App,
    },
};

#[test]
fn enable_manage_cookies() -> tsukuyomi::test::Result<()> {
    let app = App::configure(chain![
        Route::from_parts(
            "/first",
            tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                Ok("")
            })
        )?,
        Route::from_parts(
            "/second",
            tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                assert!(input.cookies.jar()?.get("session").is_some());
                Ok("")
            })
        ),
    ])?;
    let mut server = tsukuyomi::test::server(app)?;

    let mut session = server.new_session()?.save_cookies(true);
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}

#[test]
fn disable_manage_cookies() -> tsukuyomi::test::Result<()> {
    let app = App::configure(chain![
        Route::from_parts(
            "/first",
            tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                Ok("")
            })
        ),
        Route::from_parts(
            "/second",
            tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                assert!(input.cookies.jar()?.get("session").is_none());
                Ok("")
            })
        ),
    ])?;
    let mut server = tsukuyomi::test::server(app)?;

    let mut session = server.new_session()?;
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}
