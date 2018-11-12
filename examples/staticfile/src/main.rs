extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use tsukuyomi::app::App;
use tsukuyomi_fs::Staticfiles;

fn main() {
    let app = App::builder()
        .route(
            tsukuyomi::route!("/index.html") //
                .serve_file(concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html")),
        ) //
        .mount(Staticfiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")).prefix("/static")) //
        .finish()
        .unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
