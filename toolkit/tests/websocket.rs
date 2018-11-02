extern crate tsukuyomi;
extern crate tsukuyomi_toolkit;

use tsukuyomi::app::App;
use tsukuyomi::extractor::HasExtractor;
use tsukuyomi::route;
use tsukuyomi_toolkit::websocket::Ws;

#[test]
#[ignore]
fn compiletest() {
    let app = App::builder()
        .route(
            route::get("/ws")
                .with(Ws::extractor())
                .handle(|ws: Ws| Ok(ws.finish(|_| Ok(())))),
        ).finish()
        .unwrap();
    drop(app);
}
