extern crate tsukuyomi;

use tsukuyomi::app::App;

fn main() {
    let app = App::builder()
        .route(tsukuyomi::route!().reply(|| "Hello, world!\n"))
        .finish()
        .expect("failed to construct App");

    tsukuyomi::server(app)
        .run_forever()
        .expect("failed to start the server");
}
