mod integration_tests;

#[test]
#[should_panic]
fn test_catch_unwind() {
    fn inner() -> tsukuyomi::test::Result<()> {
        use tsukuyomi::app::{config::prelude::*, App};

        let app = App::create(
            path!(/) //
                .to(endpoint::any() //
                    .call(|| -> &'static str { panic!("explicit panic") })),
        )?;

        let mut server = tsukuyomi::test::server(app)?;
        server.perform("/")?;

        Ok(())
    }

    if let Err(err) = inner() {
        eprintln!("unexpected error: {:?}", err);
    }
}
