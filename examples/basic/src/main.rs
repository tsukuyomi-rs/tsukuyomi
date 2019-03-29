use tsukuyomi::{
    endpoint, //
    server::Server,
    App,
};

fn main() -> Result<(), exitfailure::ExitFailure> {
    let app = App::build(|mut scope| {
        scope
            .at("/")? //
            .to(endpoint::call(|| "Hello, world!\n"))
    })?;

    let mut server = Server::new(app)?;

    println!("Listening on http://127.0.0.1:4000/");
    server.bind("127.0.0.1:4000")?;

    server.run_forever();
    Ok(())
}
