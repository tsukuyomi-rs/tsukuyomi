extern crate tsukuyomi;

use tsukuyomi::{app::config::prelude::*, server::Server, App};

fn main() -> tsukuyomi::server::Result<()> {
    let server = App::configure(
        route().to(endpoint::any() //
            .say("Hello, world!\n")),
    ) //
    .map(Server::new)?;

    server.run()
}
