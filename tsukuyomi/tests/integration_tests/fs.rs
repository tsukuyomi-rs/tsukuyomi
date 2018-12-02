use tsukuyomi::{
    app::directives::*, //
    fs::Staticfiles,
};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::builder()
        .with(
            route("/index.html")? //
                .send_file("/path/to/index.html", None),
        ) //
        .build()
        .map(drop)
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
