extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::{App, Handler};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/").handle(Handler::new_ready(|_| "Hello, world!\n"));
        })
        .finish()?;

    tsukuyomi::run(app)
}
