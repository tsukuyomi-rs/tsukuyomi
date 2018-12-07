use tsukuyomi::{
    app::{route, App},
    fs::Staticfiles,
};

fn main() -> tsukuyomi::server::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    App::builder()
        .with(
            route::root() //
                .send_file(manifest_dir.join("static/index.html"), None),
        ) //
        .with(Staticfiles::new(manifest_dir.join("static"))) //
        .build_server()?
        .run()
}
