use {
    futures::prelude::*,
    izanami::http::response::Redirect,
    tsukuyomi::{
        config::prelude::*, //
        fs::Staticfiles,
        server::Server,
        App,
    },
    tsukuyomi_tungstenite::{Message, Ws},
    uuid::Uuid,
};

const STATIC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

fn main() -> Result<(), exitfailure::ExitFailure> {
    let app = App::create(chain![
        path!("/ws") //
            .to(endpoint::get()
                .extract(tsukuyomi_tungstenite::ws())
                .call(|ws: Ws| {
                    let conn_id = Uuid::new_v4();
                    ws.finish(move |stream| {
                        println!("[{}] Got a websocket connection.", conn_id);
                        let (tx, rx) = stream.split();
                        rx.filter_map(move |m| {
                            println!("[{}] Message from client: {:?}", conn_id, m);
                            match m {
                                Message::Ping(p) => Some(Message::Pong(p)),
                                Message::Pong(_) => None,
                                _ => Some(m),
                            }
                        }) //
                        .forward(tx)
                        .then(move |_| {
                            println!("[{}] Connection is closed.", conn_id);
                            Ok::<_, tsukuyomi::util::Never>(())
                        })
                    })
                })),
        path!("/") //
            .to(endpoint::reply(Redirect::moved_permanently(
                "/index.html".parse().expect("valid URI")
            ))),
        Staticfiles::new(STATIC_PATH)
    ])?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
