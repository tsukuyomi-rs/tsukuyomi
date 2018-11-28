#[test]
#[ignore]
fn compiletest() {
    drop(
        tsukuyomi::app!()
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
        tsukuyomi::app!()
            .with(tsukuyomi::fs::Staticfiles::new("./public")) //
            .build()
            .unwrap(),
    );
}
