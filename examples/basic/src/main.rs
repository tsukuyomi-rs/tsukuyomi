extern crate tsukuyomi;

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(tsukuyomi::route!().reply(|| "Hello, world!\n"));
    }).expect("failed to construct App");

    tsukuyomi::server(app)
        .bind("127.0.0.1:4000")
        .run_forever()
        .expect("failed to start the server");
}
