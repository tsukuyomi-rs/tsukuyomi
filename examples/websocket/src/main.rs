use {
    futures::prelude::*,
    izanami::http::response::Redirect,
    tsukuyomi::{endpoint, server::Server, App},
    tsukuyomi_tungstenite::{Message, Ws},
    uuid::Uuid,
};

// const STATIC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

fn main() -> Result<(), exitfailure::ExitFailure> {
    let app = App::builder()
        .root(|mut scope| {
            scope
                .at("/ws")?
                .get()
                .extract(tsukuyomi_tungstenite::ws())
                .to(endpoint::call(|ws: Ws| {
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
                }))?;

            let redirect_to_index =
                Redirect::moved_permanently("/index.html".parse().expect("invalid URI"));
            scope
                .at("/")?
                .get()
                .to(endpoint::call(move || redirect_to_index.clone()))?;

            //Staticfiles::new(STATIC_PATH).register(s)

            Ok(())
        })?
        .build()?;

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
