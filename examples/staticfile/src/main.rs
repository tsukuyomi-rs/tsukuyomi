extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi_fs::{NamedFile, Staticfiles};

fn main() {
    let app = App::builder()
        .route(
            tsukuyomi::route!("/index.html") //
                .handle(|| {
                    NamedFile::open(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html"))
                        .map_err(Into::into)
                }),
        ).mount(
            "/static",
            Staticfiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")),
        ).unwrap() //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
