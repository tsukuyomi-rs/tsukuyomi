extern crate ganymede;
extern crate pretty_env_logger;

use ganymede::{App, Context};

fn welcome(_cx: &Context) -> ganymede::Result<&'static str> {
    Ok("Hello, world!\n")
}

fn main() -> ganymede::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", welcome);
        })
        .finish()?;

    ganymede::run(app)
}
