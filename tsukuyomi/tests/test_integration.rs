mod integration_tests;

#[test]
#[should_panic]
fn test_catch_unwind() {
    fn inner() -> tsukuyomi::test::Result<()> {
        let mut server = tsukuyomi::App::configure(
            tsukuyomi::app::route::root() //
                .to(tsukuyomi::endpoint::any()
                    .reply(|| -> &'static str { panic!("explicit panic") })),
        )
        .map(tsukuyomi::server::Server::new)?
        .into_test_server()?;

        server.perform("/")?;

        Ok(())
    }

    if let Err(err) = inner() {
        eprintln!("unexpected error: {:?}", err);
    }
}
