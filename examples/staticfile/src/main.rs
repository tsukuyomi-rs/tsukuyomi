extern crate tsukuyomi;
extern crate tsukuyomi_fs;

use {tsukuyomi::app::route, tsukuyomi_fs::Staticfiles};

fn main() -> tsukuyomi::server::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    tsukuyomi::app!()
        .route(
            route!("/") //
                .serve_file(manifest_dir.join("static/index.html")),
        ) //
        .with(Staticfiles::new(manifest_dir.join("static"))) //
        .build_server()?
        .run()
}
