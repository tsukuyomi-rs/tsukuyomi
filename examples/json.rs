extern crate http;
extern crate tsukuyomi;
#[macro_use]
extern crate serde;
extern crate futures;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use futures::prelude::*;

use tsukuyomi::json::{Json, JsonErrorHandler};
use tsukuyomi::output::HttpResponse;
use tsukuyomi::{App, Context, Error};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
}

impl HttpResponse for User {}

fn get_json(_: &Context) -> tsukuyomi::Result<Json<User>> {
    Ok(Json(User {
        name: "Sakura Kinomoto".into(),
        age: 13,
    }))
}

fn read_json_payload(ctxt: &Context) -> impl Future<Item = Json<User>, Error = Error> + Send + 'static {
    ctxt.body().read_all().convert_to::<Json<User>>().map(|user| {
        info!("Received: {:?}", user);
        user
    })
}

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", get_json);
            r.post("/", read_json_payload);
        })
        .error_handler(JsonErrorHandler::new())
        .finish()?;

    tsukuyomi::run(app)
}
