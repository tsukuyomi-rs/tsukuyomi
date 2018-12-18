use tsukuyomi::App;
use tsukuyomi_server::Server;

fn main() -> tsukuyomi_server::Result<()> {
    let server = App::create({
        use tsukuyomi::config::prelude::*;

        path!("/") //
            .to(endpoint::any() //
                .reply("Hello, world!\n"))
    }) //
    .map(Server::new)?;

    server.run()
}
