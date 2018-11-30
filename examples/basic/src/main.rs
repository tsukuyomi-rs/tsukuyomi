extern crate tsukuyomi;

fn main() -> tsukuyomi::server::Result<()> {
    let server = tsukuyomi::App::builder()
        .with(
            tsukuyomi::app::scope::route!("/") //
                .say("Hello, world!\n"),
        ) //
        .build_server()?;

    server.run()
}
