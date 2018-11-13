extern crate serde;
extern crate tsukuyomi;

use tsukuyomi::app::route;
use tsukuyomi::extractor;

#[derive(Debug, serde::Serialize, serde::Deserialize, tsukuyomi::output::Responder)]
#[responder(respond_to = "tsukuyomi::output::responder::json")]
struct User {
    name: String,
    age: u32,
}

fn main() {
    let app = tsukuyomi::app()
        .route(
            route!("/") //
                .reply(|| {
                    tsukuyomi::output::json(User {
                        name: "Sakura Kinomoto".into(),
                        age: 13,
                    })
                }),
        ) //
        .route(
            route!("/", method = POST)
                .with(extractor::body::json())
                .reply(|user: User| user),
        ) //
        .finish()
        .expect("failed to construct App");

    tsukuyomi::server(app)
        .run_forever()
        .expect("failed to start HTTP server");
}
