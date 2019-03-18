use {
    exitfailure::ExitFailure,
    serde::{Deserialize, Serialize},
    tsukuyomi::{
        config::prelude::*, //
        extractor,
        server::Server,
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

fn main() -> Result<(), ExitFailure> {
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

    let mut server = Server::new(app)?;
    server.bind("127.0.0.1:4000")?;
    server.run_forever();

    Ok(())
}
