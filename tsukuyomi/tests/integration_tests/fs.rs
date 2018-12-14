use tsukuyomi::{
    app::config::prelude::*, //
    fs::{NamedFile, Staticfiles},
    App,
};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::create({
        path!(/"index.html") //
            .to(endpoint::get() //
                .reply(NamedFile::open("/path/to/index.html")))
    })
    .map(drop)
}

#[test]
#[ignore]
fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
    App::create(Staticfiles::new("./public")).map(drop)
}
