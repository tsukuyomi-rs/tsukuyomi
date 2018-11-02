extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_toolkit;

use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi::route;
use tsukuyomi_toolkit::fs::{NamedFile, Staticfiles};

#[test]
#[ignore]
fn compiletest() {
    let app = App::builder()
        .route(
            route::get("/index.html")
                .handle(|| NamedFile::open("/path/to/index.html").map_err(Into::into)),
        ).finish()
        .unwrap();
    drop(app);
}

#[test]
#[ignore]
fn compiletest_staticfiles() {
    let app = App::builder()
        .scope(
            Staticfiles::new("./public")
                .follow_links(true)
                .same_file_system(false)
                .filter_entry(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .map(|s| s.starts_with('.'))
                        .unwrap_or(false)
                }),
        ).finish()
        .unwrap();
    drop(app);
}
