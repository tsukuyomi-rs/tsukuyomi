extern crate futures;
extern crate tsukuyomi;

use futures::prelude::*;
use tsukuyomi::websocket::{start, OwnedMessage, Transport};
use tsukuyomi::{handler, App};

fn echo(transport: Transport) -> impl Future<Item = (), Error = ()> {
    let (tx, rx) = transport.split();
    rx.take_while(|m| Ok(!m.is_close()))
        .filter_map(|m| {
            println!("Message from client: {:?}", m);
            match m {
                OwnedMessage::Ping(p) => Some(OwnedMessage::Pong(p)),
                OwnedMessage::Pong(_) => None,
                _ => Some(m),
            }
        })
        .forward(tx)
        .and_then(|(_, tx)| tx.send(OwnedMessage::Close(None)))
        .then(|_| Ok(()))
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .route((
            "/ws",
            handler::wrap_ready(|input| start(input, |transport, _cx| echo(transport))),
        ))
        .finish()?;

    tsukuyomi::run(app)
}
