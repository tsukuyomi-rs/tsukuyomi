use tsukuyomi::{App, Server};

fn main() -> tsukuyomi::server::Result<()> {
    let server = App::create({
        use tsukuyomi::config::prelude::*;

        path!(/) //
            .to(endpoint::any() //
                .reply("Hello, world!\n"))
    }) //
    .map(Server::new)?;

    server.run()
}
