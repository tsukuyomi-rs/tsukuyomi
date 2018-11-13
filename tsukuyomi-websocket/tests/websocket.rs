extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi_websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    drop(
        tsukuyomi::app()
            .route(
                tsukuyomi::app::route!("/ws")
                    .with(tsukuyomi_websocket::extractor())
                    .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            ) //
            .finish()
            .unwrap(),
    );
}
