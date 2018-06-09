extern crate ganymede;
extern crate http;
#[macro_use]
extern crate serde;
extern crate pretty_env_logger;

use ganymede::json::Json;
use ganymede::{App, Context, Route};
use http::Method;

#[derive(Debug, Serialize)]
struct User {
    name: String,
    age: u32,
}

fn handler(_: &Context) -> ganymede::Result<Json<User>> {
    Ok(Json(User {
        name: "Sakura Kinomoto".into(),
        age: 13,
    }))
}

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", vec![Route::new("/", Method::GET, handler)])
        .finish()?;

    ganymede::run(app)
}
