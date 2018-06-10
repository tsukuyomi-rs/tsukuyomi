#![cfg_attr(rustfmt, rustfmt_skip)]

extern crate ganymede;
extern crate http;
#[macro_use]
extern crate serde;
extern crate futures;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use futures::prelude::*;
use ganymede::json::{Json, JsonErrorHandler};
use ganymede::{App, Context, Error};
use ganymede::output::HttpResponse;

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
}

impl HttpResponse for User {}

fn get_json(_: &Context) -> ganymede::Result<Json<User>> {
    Ok(Json(User {
        name: "Sakura Kinomoto".into(),
        age: 13,
    }))
}

fn read_json_payload(ctxt: &Context) -> impl Future<Item = Json<User>, Error = Error> + Send + 'static {
    ctxt.body()
        .read_all()
        .convert_to::<Json<User>>()
        .map(|user| {
            info!("Received: {:?}", user);
            user
        })
}

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", get_json);
            r.post("/", read_json_payload);
        })
        .error_handler(JsonErrorHandler::new())
        .finish()?;

    ganymede::run(app)
}
