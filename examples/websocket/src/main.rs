extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use {
    futures::prelude::*,
    tsukuyomi_websocket::{Message, Ws},
};

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!() //
        .route(
            tsukuyomi::app::route!("/ws")
                .extract(tsukuyomi_websocket::extractor())
                .reply(|ws: Ws| {
                    ws.finish(|stream| {
                        let (tx, rx) = stream.split();
                        rx.filter_map(|m| {
                            println!("Message from client: {:?}", m);
                            match m {
                                Message::Ping(p) => Some(Message::Pong(p)),
                                Message::Pong(_) => None,
                                _ => Some(m),
                            }
                        }) //
                        .forward(tx)
                        .then(|_| Ok(()))
                    })
                }),
        ) //
        .build_server()?
        .run()
}
