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
