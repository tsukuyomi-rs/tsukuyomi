use {
    izanami::Server,
    tsukuyomi::{
        config::prelude::*, //
        fs::{NamedFile, Staticfiles},
        App,
    },
};

fn main() -> izanami::Result<()> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    let app = App::create(chain![
        path!("/") //
            .to(endpoint::get() //
                .reply(NamedFile::open(manifest_dir.join("static/index.html")))),
        Staticfiles::new(manifest_dir.join("static")),
    ])?;

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 4000).into();
    let server = Server::bind_tcp(&addr)?;

    server.start(app)
}
