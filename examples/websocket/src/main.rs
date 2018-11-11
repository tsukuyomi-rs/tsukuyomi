extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use futures::prelude::*;
use tsukuyomi::app::Route;
use tsukuyomi_websocket::{Message, Ws};

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(
            Route::get("/ws")
                .with(tsukuyomi_websocket::extractor())
                .reply(|ws: Ws| {
                    ws.finish(|transport| {
                        let (tx, rx) = transport.split();
                        rx.filter_map(|m| {
                            println!("Message from client: {:?}", m);
                            match m {
                                Message::Ping(p) => Some(Message::Pong(p)),
                                Message::Pong(_) => None,
                                _ => Some(m),
                            }
                        }).forward(tx)
                        .then(|_| Ok(()))
                    })
                }),
        );
    }).unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
