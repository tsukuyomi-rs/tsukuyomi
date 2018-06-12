extern crate tsukuyomi;
extern crate pretty_env_logger;

use tsukuyomi::{App, Context};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", |_: &Context| Ok("Hello, world!\n"));
        })
        .finish()?;

    tsukuyomi::run(app)
}
