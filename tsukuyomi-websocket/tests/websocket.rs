extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi::app::{App, Route};
use tsukuyomi_websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::build(|scope| {
            scope.route(
                Route::get("/ws")
                    .with(tsukuyomi_websocket::extractor())
                    .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            );
        }).unwrap(),
    );
}
