extern crate tsukuyomi;
extern crate pretty_env_logger;

use tsukuyomi::{App, Context};

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", |_: &Context| Ok("Hello, world\n"));

            r.get("/api/:name", |cx: &Context| {
                let message = format!("Hello, {}\n", &cx.params()[0]);
                Ok(message)
            });

            r.get("/static/*path", |cx: &Context| {
                let message = format!("path = {}\n", &cx.params()[0]);
                Ok(message)
            });

            r.any("/http", |cx: &Context| Ok(format!("Method = {}", cx.method())));
        })
        .finish()?;

    tsukuyomi::run(app)
}
