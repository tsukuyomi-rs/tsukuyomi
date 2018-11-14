extern crate tsukuyomi;

fn main() {
    tsukuyomi::app()
        .route(
            tsukuyomi::app::route!() //
                .reply(|| "Hello, world!\n"),
        ).build_server()
        .expect("failed to construct App")
        .run_forever()
        .expect("failed to start the server");
}
