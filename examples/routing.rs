extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;

use ganymede::{App, Context, Route};
use http::Method;

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount(
            "/",
            vec![
                Route::new("/", Method::GET, |_: &_| Ok("Hello, world\n")),
                Route::new("/:name", Method::GET, |cx: &Context| {
                    let params = cx.params().expect("cx.params() is empty");
                    let message = format!("Hello, {}\n", &params[0]);
                    Ok(message)
                }),
            ],
        )
        .finish()?;

    ganymede::run(app)
}
