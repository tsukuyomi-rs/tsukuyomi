extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::future::ready;
use tsukuyomi::{App, Context};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            // r.get("/", async |_: &_| "Hello, world\n");
            r.get("/", |_| ready("Hello, world\n"));

            r.get("/api/:name", |cx: &Context| {
                ready(format!("Hello, {}\n", &cx.params()[0]))
            });

            r.get("/static/*path", |cx: &Context| {
                ready(format!("path = {}\n", &cx.params()[0]))
            });
        })
        .finish()?;

    tsukuyomi::run(app)
}
