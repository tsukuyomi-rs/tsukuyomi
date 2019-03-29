use tsukuyomi::{endpoint, fs::NamedFile, App};

#[test]
#[ignore]
fn compiletest() -> tsukuyomi::app::Result<()> {
    App::build(|mut s| {
        s.at("/index.html")?
            .get()
            .to(endpoint::call(|| NamedFile::open("/path/to/index.html")))
    })
    .map(|_: App| ())
}

// #[test]
// #[ignore]
// fn compiletest_staticfiles() -> tsukuyomi::app::Result<()> {
//     App::build(|s| Staticfiles::new("./public").register(s)).map(|_: App| ())
// }
