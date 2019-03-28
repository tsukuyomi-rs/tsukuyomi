use tsukuyomi::{
    endpoint::builder as endpoint, //
    server::Server,
    App,
};

fn main() -> Result<(), exitfailure::ExitFailure> {
    let app = App::build(|s| {
        s.at("/", (), {
            endpoint::reply("Hello, world!\n") //
        })
    })?;

    let mut server = Server::new(app)?;

    println!("Listening on http://127.0.0.1:4000/");
    server.bind("127.0.0.1:4000")?;

    server.run_forever();
    Ok(())
}
