extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi::app::{App, Route};
use tsukuyomi_websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    drop(
        App::builder()
            .route(
                Route::get("/ws")
                    .unwrap()
                    .with(tsukuyomi_websocket::extractor())
                    .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            ) //
            .finish()
            .unwrap(),
    );
}
