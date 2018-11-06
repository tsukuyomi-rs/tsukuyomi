extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::route;
use tsukuyomi_fs::{NamedFile, Staticfiles};

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route({
            route::get("/index.html").handle(|| {
                NamedFile::open(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html"))
                    .map_err(Into::into)
            })
        });

        scope.mount("/static", |scope| {
            Staticfiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")).register(scope);
        });
    }).unwrap();

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .unwrap();
}
