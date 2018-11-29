extern crate serde;
extern crate tsukuyomi;

use {
    serde::{Deserialize, Serialize},
    tsukuyomi::{extractor, route, Responder},
};

#[derive(Clone, Debug, Serialize, Deserialize, Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct User {
    name: String,
    age: u32,
}

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::App::builder()
        .with(
            route!("/") //
                .say(User {
                    name: "Sakura Kinomoto".into(),
                    age: 13,
                }),
        ) //
        .with(
            route!("/", method = POST)
                .extract(extractor::body::json())
                .reply(|user: User| user),
        ) //
        .build_server()?
        .run()
}
