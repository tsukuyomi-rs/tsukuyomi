extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi::app::App;
use tsukuyomi::route;
use tsukuyomi_websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::build(|scope| {
            scope.route(
                route::get("/ws")
                    .with(tsukuyomi_websocket::extractor())
                    .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            );
        }).unwrap(),
    );
}
