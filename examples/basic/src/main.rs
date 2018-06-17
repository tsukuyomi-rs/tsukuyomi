extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/").handle(|_| "Hello, world!\n");
        })
        .finish()?;

    tsukuyomi::run(app)
}
