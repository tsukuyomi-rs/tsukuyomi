use tsukuyomi::{
    app::{route, App},
    fs::Staticfiles,
};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::builder()
        .with(
            route::root()
                .segment("index.html")?
                .send_file("/path/to/index.html", None),
        ) //
        .build()
        .map(drop)
}

#[test]
#[ignore]
fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
    App::builder()
        .with(Staticfiles::new("./public"))
        .build()
        .map(drop)
}
