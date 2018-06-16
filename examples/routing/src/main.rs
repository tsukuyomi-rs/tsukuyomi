extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::future::ready;
use tsukuyomi::{App, Input};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            // r.get("/", async || "Hello, world\n");
            r.get("/", || ready("Hello, world\n"));

            r.get("/api/:name", || {
                ready(Input::with(|input| format!("Hello, {}\n", &input.params()[0])))
            });

            r.get("/static/*path", || {
                ready(Input::with(|input| format!("path = {}\n", &input.params()[0])))
            });
        })
        .finish()?;

    tsukuyomi::run(app)
}
