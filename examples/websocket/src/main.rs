use {
    futures::prelude::*,
    tsukuyomi::{
        config::prelude::*, //
        fs::Staticfiles,
        output::redirect,
        server::Server,
        App,
    },
    tsukuyomi_tungstenite::{Message, Ws},
};

const STATIC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

fn main() -> Result<(), exitfailure::ExitFailure> {
    let app = App::create(chain![
        path!("/ws") //
            .to(endpoint::get().call(|| Ws::new(|stream| {
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
                .map(|_| ())
            }))),
        path!("/") //
            .to(endpoint::reply(redirect::to("/index.html"))),
        Staticfiles::new(STATIC_PATH)
    ])?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
