extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_websocket;

use {
    futures::prelude::*,
    tsukuyomi_websocket::{Message, Ws},
};

fn main() -> tsukuyomi::server::Result<()> {
    let handle_websocket = tsukuyomi::app::route!("/ws")
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
        });

    let server = tsukuyomi::app() //
        .route(handle_websocket)
        .build_server()?;

    server.run_forever()
}
