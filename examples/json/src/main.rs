use {
    izanami::Server,
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        App,
        IntoResponse,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, IntoResponse)]
#[response(preset = "tsukuyomi::output::preset::Json")]
struct User {
    name: String,
    age: u32,
}

fn main() -> izanami::Result<()> {
    let app = App::create(
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
    )?;

    let server = Server::bind_tcp(&"127.0.0.1:4000".parse()?)?;
    server.start(app)
}
