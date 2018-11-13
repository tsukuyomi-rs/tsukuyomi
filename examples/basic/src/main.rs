extern crate tsukuyomi;

fn main() {
    let app = tsukuyomi::app()
        .route(tsukuyomi::app::route!().reply(|| "Hello, world!\n"))
        .finish()
        .expect("failed to construct App");

    tsukuyomi::server(app)
        .run_forever()
        .expect("failed to start the server");
}
