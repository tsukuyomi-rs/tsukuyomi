use {
    tsukuyomi::{
        config::prelude::*, //
        fs::{NamedFile, Staticfiles},
        App,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    App::create(chain![
        path!("/") //
            .to(endpoint::get() //
                .reply(NamedFile::open(manifest_dir.join("static/index.html")))),
        Staticfiles::new(manifest_dir.join("static")),
    ])
    .map(App::into_service)
    .map(Server::new)?
    .run()
}
