use tsukuyomi::{
    app::directives::*, //
    fs::Staticfiles,
};

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::builder()
            .with(
                route!("/index.html") //
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
            .with(Staticfiles::new("./public")) //
            .build()
            .unwrap(),
    );
}
