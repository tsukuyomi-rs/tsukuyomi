extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi_fs::{NamedFile, Staticfiles};

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

#[test]
#[ignore]
fn compiletest_staticfiles() {
    drop(
        App::builder()
            .mount(
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
            ) //
            .finish()
            .unwrap(),
    );
}
