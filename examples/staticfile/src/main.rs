extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi::fs::NamedFile;
use tsukuyomi_fs::Staticfiles;

fn main() {
    let app = App::builder()
        .route(
            tsukuyomi::route!("/index.html") //
                .handle(|| {
                    NamedFile::open(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html"))
                        .map_err(Into::into)
                }),
        ) //
        .mount(Staticfiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")).prefix("/static")) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
