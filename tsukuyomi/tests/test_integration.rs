mod integration_tests;

#[test]
#[should_panic]
fn test_catch_unwind() {
    fn inner() -> tsukuyomi::test::Result<()> {
        let app = tsukuyomi::App::configure(
            tsukuyomi::app::config::route::route() //
                .to(tsukuyomi::app::config::endpoint::any()
                    .reply(|| -> &'static str { panic!("explicit panic") })),
        )?;

        let mut server = tsukuyomi::test::server(app)?;
        server.perform("/")?;

        Ok(())
    }

    if let Err(err) = inner() {
        eprintln!("unexpected error: {:?}", err);
    }
}
