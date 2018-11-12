use tsukuyomi::app::App;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::builder()
            .route(
                tsukuyomi::route!("/index.html") //
                    .serve_file("/path/to/index.html"),
            ) //
            .finish()
            .unwrap(),
    );
}
