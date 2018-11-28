extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_tungstenite;

use {
    futures::prelude::*,
    tsukuyomi_tungstenite::{Message, Ws},
};

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!() //
        .with(
            tsukuyomi::app::route!("/ws")
                .extract(tsukuyomi_tungstenite::ws())
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
