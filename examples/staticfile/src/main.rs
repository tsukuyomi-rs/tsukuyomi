extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use futures::prelude::*;
use tsukuyomi::app::App;
use tsukuyomi::route;
use tsukuyomi_fs::{NamedFile, Staticfiles};

fn main() {
    let app = App::builder();

    let app = app.route({
        route::get("/index.html").handle(|| {
            NamedFile::open(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html"))
                .map_err(Into::into)
        })
    });

    let app = app.mount("/static", |s| {
        s.scope(Staticfiles::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/static"
        )));
    });

    let app = app.finish().unwrap();

    tsukuyomi::server::server(app)
        .transport(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
        .run_forever()
        .unwrap();
}
