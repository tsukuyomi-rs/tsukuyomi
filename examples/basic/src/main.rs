extern crate tsukuyomi;

use tsukuyomi::app::App;
use tsukuyomi::route;

fn main() {
    let app = App::builder()
        .route(route::index().reply(|| "Hello, world!\n"))
        .finish()
        .expect("failed to construct App");

    tsukuyomi::server::server(app)
        .transport("127.0.0.1:4000")
        .run_forever()
        .expect("failed to start the server");
}
