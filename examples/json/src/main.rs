use {
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        app::config::prelude::*, //
        chain,
        extractor,
        server::Server,
        App,
        Responder,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct User {
    name: String,
    age: u32,
}

fn main() -> tsukuyomi::server::Result<()> {
    App::configure(
        route() //
            .to(chain![
                endpoint::get().say(User {
                    name: "Sakura Kinomoto".into(),
                    age: 13,
                }),
                endpoint::post()
                    .extract(extractor::body::json())
                    .reply(|user: User| user),
            ]),
    )
    .map(Server::new)?
    .run()
}
