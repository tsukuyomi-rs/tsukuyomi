#![feature(proc_macro, proc_macro_non_items, generators)]

extern crate tsukuyomi;
#[macro_use]
extern crate serde;
extern crate futures_await as futures;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use futures::prelude::*;

use tsukuyomi::json::{Json, JsonErrorHandler};
use tsukuyomi::output::HttpResponse;
use tsukuyomi::{App, Input};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
}

impl HttpResponse for User {}

#[async]
fn async_handler() -> tsukuyomi::Result<Json<User>> {
    let user: Json<User> = await!(Input::with_get(|input| input.body_mut().read_all()).convert_to())?;
    info!("Received: {:?}", user);
    Ok(user)
}

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.post("/").handle_async(async_handler);
        })
        .error_handler(JsonErrorHandler::new())
        .finish()?;

    tsukuyomi::run(app)
}
