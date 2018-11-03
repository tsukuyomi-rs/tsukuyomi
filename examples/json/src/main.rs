extern crate serde;
extern crate tsukuyomi;

use tsukuyomi::app::App;
use tsukuyomi::route;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
    age: u32,
}

fn main() {
    let app = App::builder()
        .route(route::get("/").reply(|| {
            tsukuyomi::output::json(User {
                name: "Sakura Kinomoto".into(),
                age: 13,
            })
        })).route(
            route::post("/")
                .with(tsukuyomi::extractor::body::json())
                .reply(|user: User| tsukuyomi::output::json(user)),
        ).finish()
        .unwrap();

    tsukuyomi::server::server(app)
        .transport(std::net::SocketAddr::from(([127, 0, 0, 1], 4000)))
        .run_forever()
        .unwrap();
}
