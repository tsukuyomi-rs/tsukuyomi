use {
    exitfailure::ExitFailure,
    tsukuyomi::{
        endpoint::builder as endpoint,
        fs::{NamedFile, Staticfiles},
        server::Server,
        App,
    },
};

fn main() -> Result<(), ExitFailure> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    let app = App::build(|s| {
        s.at("/", (), {
            endpoint::get() //
                .reply(NamedFile::open(manifest_dir.join("static/index.html")))
        })?;

        s.add(Staticfiles::new(manifest_dir.join("static")))
    })?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
