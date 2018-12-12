use tsukuyomi::{
    app::config::prelude::*, //
    fs::Staticfiles,
    App,
};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::create({
        route()
            .segment("index.html")?
            .to(endpoint::get().send_file("/path/to/index.html", None))
    })
    .map(drop)
}

#[test]
#[ignore]
fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
    App::create(Staticfiles::new("./public")).map(drop)
}
