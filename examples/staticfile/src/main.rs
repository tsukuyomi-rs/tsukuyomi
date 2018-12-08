use tsukuyomi::{
    app::config::prelude::*, //
    chain,
    fs::Staticfiles,
    server::Server,
    App,
};

fn main() -> tsukuyomi::server::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    App::configure(chain![
        route::root() //
            .send_file(manifest_dir.join("static/index.html"), None),
        Staticfiles::new(manifest_dir.join("static")),
    ])
    .map(Server::new)?
    .run()
}
