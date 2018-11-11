extern crate serde;
extern crate tsukuyomi;

use tsukuyomi::app::{App, Route};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
    age: u32,
}

fn main() {
    let app = App::builder()
        .route(
            Route::get("/") //
                .reply(|| {
                    tsukuyomi::output::json(User {
                        name: "Sakura Kinomoto".into(),
                        age: 13,
                    })
                }),
        ) //
        .route(
            Route::post("/")
                .with(tsukuyomi::extractor::body::json::<User>())
                .reply(tsukuyomi::output::json),
        ) //
        .finish()
        .expect("failed to construct HTTP server");

    tsukuyomi::server(app)
        .run_forever()
        .expect("failed to start HTTP server");
}
