#[test]
#[ignore]
fn compiletest() {
    drop(
        tsukuyomi::app!()
            .route(
                tsukuyomi::route!("/index.html") //
                    .serve_file("/path/to/index.html"),
            ) //
            .build()
            .unwrap(),
    );
}

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        tsukuyomi::app!()
            .with(tsukuyomi::fs::Staticfiles::new("./public")) //
            .build()
            .unwrap(),
    );
}
