extern crate futures;
extern crate tsukuyomi;
extern crate tsukuyomi_tungstenite;

use {
    futures::prelude::*,
    tsukuyomi::app::directives::*,
    tsukuyomi_tungstenite::{ws, Message, Ws},
};

fn main() -> tsukuyomi::server::Result<()> {
    App::builder() //
        .with(
            route!("/ws") //
                .extract(ws())
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
