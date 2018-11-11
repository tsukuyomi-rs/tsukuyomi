extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::app::{App, Route};
use tsukuyomi_fs::{NamedFile, Staticfiles};

fn main() {
    let app = App::builder()
        .route(
            Route::get("/index.html") //
                .handle(|| {
                    NamedFile::open(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html"))
                        .map_err(Into::into)
                }),
        ).mount("/static", |scope| {
            Staticfiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")) //
                .register(scope)
        }).unwrap() //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
