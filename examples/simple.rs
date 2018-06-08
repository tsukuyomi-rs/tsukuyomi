extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;

use ganymede::{App, Context, Error, Route};
use http::Method;

fn welcome(_cx: &Context) -> Result<&'static str, Error> {
    Ok("Hello")
}

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", vec![Route::new("/", Method::GET, welcome)])
        .finish()?;

    ganymede::run(app)
}
