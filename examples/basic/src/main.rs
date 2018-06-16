extern crate pretty_env_logger;
extern crate tsukuyomi;

use tsukuyomi::future::ready;
use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", || ready("Hello, world!\n"));
        })
        .finish()?;

    tsukuyomi::run(app)
}
