use {
    exitfailure::ExitFailure,
    tsukuyomi::{endpoint, fs::NamedFile, server::Server, App},
};

fn main() -> Result<(), ExitFailure> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    let app = App::builder()
        .root(|mut scope| {
            scope.at("/")?.get().to(endpoint::call(move || {
                NamedFile::open(manifest_dir.join("static/index.html"))
            }))?;

            //Staticfiles::new(manifest_dir.join("static")).register(s)

            Ok(())
        })?
        .build()?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
