use {
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        App,
        IntoResponse,
        Server,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, IntoResponse)]
#[response(with = "tsukuyomi::output::into_response::json")]
struct User {
    name: String,
    age: u32,
}

fn main() -> tsukuyomi::server::Result<()> {
    App::create(
        path!("/") //
            .to(chain![
                endpoint::get().reply(User {
                    name: "Sakura Kinomoto".into(),
                    age: 13,
                }),
                endpoint::post()
                    .extract(extractor::body::json())
                    .call(|user: User| user),
            ]),
    )
    .map(Server::new)?
    .run()
}
