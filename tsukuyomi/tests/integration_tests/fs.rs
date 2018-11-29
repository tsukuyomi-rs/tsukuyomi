use tsukuyomi::App;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::builder()
            .with(
                tsukuyomi::route!("/index.html") //
                    .send_file("/path/to/index.html", None),
            ) //
            .build()
            .unwrap(),
    );
}

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        App::builder()
            .with(tsukuyomi::fs::Staticfiles::new("./public")) //
            .build()
            .unwrap(),
    );
}
