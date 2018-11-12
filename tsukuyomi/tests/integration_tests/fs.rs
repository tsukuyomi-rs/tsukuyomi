use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi::fs::NamedFile;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::builder()
            .route(
                tsukuyomi::route!("/index.html")
                    .handle(|| NamedFile::open("/path/to/index.html").map_err(Into::into)),
            ) //
            .finish()
            .unwrap(),
    );
}
