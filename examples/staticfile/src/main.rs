use tsukuyomi::{
    config::prelude::*, //
    fs::{NamedFile, Staticfiles},
    App,
    Server,
};

fn main() -> tsukuyomi::server::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    App::create(chain![
        path!(/) //
            .to(endpoint::get() //
                .reply(NamedFile::open(manifest_dir.join("static/index.html")))),
        Staticfiles::new(manifest_dir.join("static")),
    ])
    .map(Server::new)?
    .run()
}
