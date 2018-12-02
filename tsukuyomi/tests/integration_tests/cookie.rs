use {
    cookie::Cookie,
    tsukuyomi::{
        app::directives::*, //
        handler::AsyncResult,
        Output,
    },
};

#[test]
fn enable_manage_cookies() -> tsukuyomi::test::Result<()> {
    let mut server = App::builder()
        .with(
            route("/first")? //
                .raw(|| {
                    AsyncResult::ready(|input| {
                        input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                        Ok(Output::default())
                    })
                }),
        ) //
        .with(
            route("/second")? //
                .raw(|| {
                    AsyncResult::ready(|input| {
                        assert!(input.cookies.jar()?.get("session").is_some());
                        Ok(Output::default())
                    })
                }),
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
            route("/first")? //
                .raw(|| {
                    AsyncResult::ready(|input| {
                        input.cookies.jar()?.add(Cookie::new("session", "xxxx"));
                        Ok(Output::default())
                    })
                }),
        ) //
        .with(
            route("/second")? //
                .raw(|| {
                    AsyncResult::ready(|input| {
                        assert!(input.cookies.jar()?.get("session").is_none());
                        Ok(Output::default())
                    })
                }),
        ) //
        .build_server()?
        .into_test_server()?;

    let mut session = server.new_session()?;
    let _ = session.perform("/first")?;
    let _ = session.perform("/second")?;

    Ok(())
}
