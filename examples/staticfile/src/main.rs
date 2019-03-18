use {
    exitfailure::ExitFailure,
    tsukuyomi::{
        config::prelude::*, //
        fs::{NamedFile, Staticfiles},
        server::Server,
        App,
    },
};

fn main() -> Result<(), ExitFailure> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    let app = App::create(chain![
        path!("/") //
            .to(endpoint::get() //
                .reply(NamedFile::open(manifest_dir.join("static/index.html")))),
        Staticfiles::new(manifest_dir.join("static")),
    ])?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
