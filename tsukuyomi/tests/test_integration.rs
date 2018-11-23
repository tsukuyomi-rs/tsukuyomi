extern crate cargo_version_sync;
extern crate cookie;
extern crate either;
extern crate futures;
extern crate http;
extern crate serde;
extern crate time;
extern crate tsukuyomi;

mod integration_tests;

#[test]
fn test_version_sync() {
    cargo_version_sync::assert_version_sync();
}

#[test]
#[should_panic]
fn test_catch_unwind() {
    fn inner() -> tsukuyomi::test::Result<()> {
        let mut server = tsukuyomi::app!()
            .route(
                tsukuyomi::route!("/") //
                    .reply(|| -> &'static str { panic!("explicit panic") }),
            ) //
            .build_server()?
            .into_test_server()?;

        server.perform("/")?;

        Ok(())
    }

    if let Err(err) = inner() {
        eprintln!("unexpected error: {:?}", err);
    }
}
