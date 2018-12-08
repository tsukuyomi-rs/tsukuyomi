use tsukuyomi::{app::config::prelude::*, fs::Staticfiles, App};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::configure({
        route::root()
            .segment("index.html")?
            .send_file("/path/to/index.html", None)
    })
    .map(drop)
}

#[test]
#[ignore]
fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
    App::configure(Staticfiles::new("./public")).map(drop)
}
