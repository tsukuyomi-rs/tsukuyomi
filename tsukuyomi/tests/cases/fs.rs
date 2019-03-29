use tsukuyomi::{
    endpoint::builder as endpoint,
    fs::{NamedFile, Staticfiles},
    App,
};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::build(|s| {
        s.at("/index.html", (), {
            endpoint::get() //
                .reply(NamedFile::open("/path/to/index.html"))
        })
    })
    .map(|_: App| ())
}

#[test]
#[ignore]
fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
    App::build(|s| Staticfiles::new("./public").register(s)).map(|_: App| ())
}
