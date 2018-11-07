extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi::app::App;
use tsukuyomi::extractor::HasExtractor;
use tsukuyomi::route;
use tsukuyomi_websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::build(|scope| {
            scope.route(
                route::get("/ws")
                    .with(Ws::extractor())
                    .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            );
        }).unwrap(),
    );
}
