extern crate tsukuyomi;

fn main() -> tsukuyomi::server::Result<()> {
    let server = tsukuyomi::app!()
        .route(
            tsukuyomi::app::route!() //
                .say("Hello, world!\n"),
        ) //
        .build_server()?;

    server.run()
}
