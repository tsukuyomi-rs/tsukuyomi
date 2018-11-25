extern crate cargo_version_sync;
extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use tsukuyomi_websocket::Ws;

#[test]
fn test_version_sync() {
    cargo_version_sync::assert_version_sync();
}

#[test]
#[ignore]
fn compiletest() {
    drop(
        tsukuyomi::app!()
            .route(
                tsukuyomi::app::route!("/ws")
                    .extract(tsukuyomi_websocket::extractor())
                    .call(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
            ) //
            .build()
            .unwrap(),
    );
}
