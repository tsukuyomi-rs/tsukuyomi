extern crate serde;
extern crate tsukuyomi;

use tsukuyomi::{app::route, extractor};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, tsukuyomi::output::Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct User {
    name: String,
    age: u32,
}

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app!()
        .route(
            route!("/") //
                .say(User {
                    name: "Sakura Kinomoto".into(),
                    age: 13,
                }),
        ) //
        .route(
            route!("/", method = POST)
                .extract(extractor::body::json())
                .reply(|user: User| user),
        ) //
        .build_server()?
        .run()
}
