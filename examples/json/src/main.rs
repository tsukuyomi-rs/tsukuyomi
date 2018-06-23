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
use tsukuyomi::{App, Error, Handler, Input};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
}

impl HttpResponse for User {}

fn get_json(_: &mut Input) -> Json<User> {
    Json(User {
        name: "Sakura Kinomoto".into(),
        age: 13,
    })
}

// async fn read_json_payload() -> tsukuyomi::Result<Json<User>> { ... }
fn read_json_payload() -> impl Future<Item = Json<User>, Error = Error> + Send + 'static {
    Input::with_get(|input| input.body_mut().read_all())
        .convert_to::<Json<User>>()
        .map(|user| {
            info!("Received: {:?}", user);
            user
        })
}

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/").handle(Handler::new_ready(get_json));
            r.post("/").handle(Handler::new_fully_async(read_json_payload));
        })
        .error_handler(JsonErrorHandler::new())
        .finish()?;

    tsukuyomi::run(app)
}
