extern crate tsukuyomi;

use tsukuyomi::app::directives::*;

fn main() -> tsukuyomi::server::Result<()> {
    let server = App::builder()
        .with(
            route!("/") //
                .say("Hello, world!\n"),
        ) //
        .build_server()?;

    server.run()
}
