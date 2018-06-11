extern crate ganymede;
extern crate pretty_env_logger;

use ganymede::{App, Context};

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", |_: &Context| Ok("Hello, world\n"));

            r.get("/:name", |cx: &Context| {
                let message = format!("Hello, {}\n", &cx.params()[0]);
                Ok(message)
            });
        })
        .finish()?;

    ganymede::run(app)
}
