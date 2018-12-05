use {
    cookie::Cookie,
    tsukuyomi::app::{route, App},
};

#[test]
fn enable_manage_cookies() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route::root()
                .segment("first")?
                .raw(tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                    input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                    Ok("")
                })),
        ) //
        .with(
            route::root()
                .segment("second")?
                .raw(tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                    assert!(input.cookies.jar()?.get("session").is_some());
                    Ok("")
                })),
        ) //
        .build_server()?
        .into_test_server()?;

    let mut session = server.new_session()?.save_cookies(true);
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}

#[test]
fn disable_manage_cookies() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route::root()
                .segment("first")?
                .raw(tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                    input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                    Ok("")
                })),
        ) //
        .with(
            route::root()
                .segment("second")?
                .raw(tsukuyomi::handler::ready(|input| -> tsukuyomi::Result<_> {
                    assert!(input.cookies.jar()?.get("session").is_none());
                    Ok("")
                })),
        ) //
        .build_server()?
        .into_test_server()?;

    let mut session = server.new_session()?;
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}
